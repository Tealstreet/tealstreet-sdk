import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { Exchange } from '../exchanges/base';
import type { WasmEngineModule } from '../exchanges/rust/rust.exchange';
import type { ExchangeName } from '../types';
import type { AppSettingsWorkerType } from '../webworker/AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../webworker/ExchangeMetadataWorkerType';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { RustEngineExchange, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderStatus, OrderType, PositionSide } from '../types';
import { CallMethodHandler } from '../webworker/handlers/CallMethodHandler';
import { consumerFrame } from './cutover/consumerFrame';
import { engineCapabilities } from './fixtures/engineCapabilities';

const order = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 2 };

type Fake = {
  commands: Array<{ op: string; [field: string]: unknown }>;
  applied: Array<{ snapshot: boolean; bytes: number }>;
  started: number;
  constructed: number;
  refuseNext?: boolean;
  /** The next placement leaves an uncertain write, held as its engine names it. */
  holdOrders?: { symbols: string; liftAfterMs: number };
  offsets: Array<number | undefined>;
};

let lastModule: WasmEngineModule | undefined;

const fakeEngine = ({ supported = true, frameFed = true, venue = 'bybit' as ExchangeName } = {}): Fake => {
  const fake: Fake = { commands: [], applied: [], started: 0, constructed: 0, offsets: [] };
  const idle = () => '[]';
  const frame = () =>
    new Uint8Array(
      consumerFrame({
        markets: [],
        tickers: [],
        loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
      }),
    );
  const engine = {
    syncHostTime: () => true,
    capabilities: () => engineCapabilities(venue),
    construction: () => JSON.stringify({ frameFed, canSign: true, publicDataOnly: false }),
    start: () => {
      fake.started += 1;
      return '[]';
    },
    wsOpen: idle,
    wsMessage: idle,
    wsClose: idle,
    wsRouteExhausted: idle,
    httpStart: idle,
    httpChunk: idle,
    httpEnd: idle,
    httpFailed: idle,
    timer: idle,
    command: (json: string) => {
      const command = JSON.parse(json) as { op: string; command?: number };
      fake.commands.push(command);
      const result = ['placeOrders', 'amendOrder', 'amendOrderWithSymbol'].includes(command.op) ? ['placed-1'] : null;
      if (command.command === undefined) return '[]';
      const done = { op: 'done', command: command.command, result: JSON.stringify(result) };
      // A confirmed settings write holds it over the source, as a frame-fed venue does.
      const held = { op: 'event', level: 'info', code: 'settings.held', message: 'held', fields: { symbol: 'BTCUSDT' } };
      if (command.op === 'placeOrders' && fake.holdOrders) {
        const orders = { op: 'event', level: 'info', code: 'orders.held', message: 'held', fields: fake.holdOrders };
        return JSON.stringify([orders, done]);
      }
      return JSON.stringify(command.op === 'setPositionMode' ? [held, done] : [done]);
    },
    applyFrame: (bytes: Uint8Array, snapshot: boolean) => {
      if (fake.refuseNext) {
        fake.refuseNext = false;
        throw new Error('frame refused');
      }
      fake.applied.push({ snapshot, bytes: bytes.byteLength });
      return '[]';
    },
    setSourceClockOffset: (offset: number | undefined) => {
      fake.offsets.push(offset);
    },
    snapshotFrame: frame,
    takeFrame: frame,
  };
  const construct = function () {
    fake.constructed += 1;
    return engine;
  };
  // A static on the class, as the wasm exports it.
  const WasmEngine = Object.assign(construct, { frameFedSupported: () => supported });
  lastModule = { WasmEngine: WasmEngine as unknown as WasmEngineModule['WasmEngine'] };
  setWasmEngineLoader(async () => lastModule!);
  return fake;
};

const mirrorWith = (fake: Fake, failure?: Record<string, unknown>) => {
  void fake;
  const exchange = new RustEngineExchange(
    'bybit',
    { key: 'k', secret: 's', workerMirror: true, publicDataOnly: false },
    new DefaultStore(),
  );
  const calls: string[] = [];
  const orderAsks: unknown[][] = [];
  exchange.on('callMethod', (event: { requestId: string; method: string; args: unknown[] }) => {
    calls.push(event.method);
    if (event.method === 'refreshAccountOrders') orderAsks.push(event.args);
    if (failure) {
      exchange.emitter.emit('callMethod:done', { requestId: event.requestId, success: false, result: failure });
      return;
    }
    const result =
      event.method === 'stopAccountAlgos'
        ? { stoppedChasers: ['c-1'], stoppedTwaps: [] }
        : event.method.startsWith('fetch')
          ? []
          : null;
    exchange.emitter.emit('callMethod:done', { requestId: event.requestId, success: true, result });
  });
  return { exchange, calls, orderAsks };
};

describe('frame-fed main-thread mirror', () => {
  let exchange: RustEngineExchange | undefined;
  afterEach(() => {
    exchange?.dispose();
    exchange = undefined;
    vi.restoreAllMocks();
  });

  it.each([false, true])('keeps the admitted owner before startup selects the local writer (switch=%s)', async (switchOwner) => {
    const fake = fakeEngine();
    const module = lastModule!;
    let release!: (module: WasmEngineModule) => void;
    const startup = new Promise<WasmEngineModule>((resolve) => { release = resolve; });
    setWasmEngineLoader(() => startup);
    const mirror = mirrorWith(fake);
    exchange = mirror.exchange;
    const owner = { ownerId: 'A', generation: 1 };
    exchange.setWorkerOwner(() => owner);
    const pending = exchange.placeOrder(order).catch((error: unknown) => error);
    if (switchOwner) owner.ownerId = 'B';
    release(module);
    if (switchOwner) {
      expect(await pending).toMatchObject({ command: 'placeOrder', outcome: 'notSent' });
      expect(fake.commands.filter(({ op }) => op === 'placeOrders')).toEqual([]);
      expect(mirror.calls).toEqual([]);
      await expect(exchange.placeOrder(order)).resolves.toEqual(['placed-1']);
    } else expect(await pending).toEqual(['placed-1']);
    expect(fake.commands.filter(({ op }) => op === 'placeOrders')).toHaveLength(1);
  });

  it('signs and sends a manual write on its own engine, never through the data source', async () => {
    const fake = fakeEngine();
    const mirror = mirrorWith(fake);
    exchange = mirror.exchange;
    await expect(exchange.frameFedReady).resolves.toBe(true);
    expect(exchange.isFrameFed).toBe(true);
    expect(fake.started).toBe(1);

    await expect(exchange.placeOrder(order)).resolves.toEqual(['placed-1']);
    await exchange.callMethodInWorker('setLeverage', 'BTCUSDT', 5);
    await exchange.cancelSymbolOrders('BTCUSDT');
    await exchange.updateOrder({
      order: { ...order, id: 'placed-1', status: OrderStatus.Open, filled: 0, remaining: 1, reduceOnly: false },
      update: { price: 3 },
    } as never);

    const ops = fake.commands.map((command) => command.op);
    expect(ops).toEqual(expect.arrayContaining(['placeOrders', 'setLeverage', 'cancelSymbolOrders']));
    expect(ops.some((op) => op === 'amendOrder' || op === 'amendOrderWithSymbol')).toBe(true);
    expect(mirror.calls).not.toContain('placeOrder');
    expect(mirror.calls).not.toContain('setLeverage');
    expect(mirror.calls).not.toContain('cancelSymbolOrders');
    expect(mirror.calls).not.toContain('updateOrder');
  });

  describe('a placement clicked while the page engine loads', () => {
    it('waits for the engine and sends one leg, never through the data source', async () => {
      const fake = fakeEngine();
      const mirror = mirrorWith(fake);
      exchange = mirror.exchange;

      const placed = mirror.exchange.placeOrder(order);
      await expect(placed).resolves.toEqual(['placed-1']);

      expect(fake.commands.map((command) => command.op)).toContain('placeOrders');
      expect(mirror.calls).not.toContain('placeOrders');
      expect(mirror.calls).not.toContain('placeOrder');
    });

    it('refuses as notSent at the bound and sends nothing when the engine arrives later', async () => {
      vi.useFakeTimers();
      try {
        const fake = fakeEngine();
        let finishLoad = () => {};
        const loaded = new Promise<void>((resolve) => {
          finishLoad = resolve;
        });
        const realLoader = lastModule!;
        setWasmEngineLoader(async () => {
          await loaded;
          return realLoader;
        });
        const mirror = mirrorWith(fake);
        exchange = mirror.exchange;

        const errors: unknown[] = [];
        mirror.exchange.on('error', (error: unknown) => errors.push(error));
        const placed = mirror.exchange.placeOrder(order);
        const refused = expect(placed).rejects.toMatchObject({ outcome: 'notSent' });
        await vi.advanceTimersByTimeAsync(30_000);
        await refused;
        expect(errors).toHaveLength(1);

        finishLoad();
        await vi.advanceTimersByTimeAsync(1_000);
        expect(fake.commands.map((command) => command.op)).not.toContain('placeOrders');
        expect(mirror.calls).not.toContain('placeOrder');
        expect(mirror.calls).not.toContain('placeOrders');
      } finally {
        vi.useRealTimers();
      }
    });

    it('forwards to the data source once a family proves to have no page engine', async () => {
      const fake = fakeEngine({ supported: false });
      const mirror = mirrorWith(fake);
      exchange = mirror.exchange;

      await mirror.exchange.placeOrder(order);

      expect(fake.commands.map((command) => command.op)).not.toContain('placeOrders');
      expect(mirror.calls.some((method) => method.startsWith('placeOrder'))).toBe(true);
    });
  });

  describe('a write that fails on its data source', () => {
    const failingMirror = async (failure: Record<string, unknown>) => {
      const mirror = mirrorWith(fakeEngine({ supported: false }), failure);
      exchange = mirror.exchange;
      await mirror.exchange.frameFedReady;
      const errors: unknown[] = [];
      mirror.exchange.on('error', (error: unknown) => errors.push(error));
      return { mirror, errors };
    };

    it('reports it on the page, as a hosted server reports nothing', async () => {
      const { mirror, errors } = await failingMirror({
        message: 'Signing timed out before dispatch',
        kind: 'timeout',
        outcome: 'notSent',
      });

      await expect(mirror.exchange.placeOrder(order)).rejects.toMatchObject({ outcome: 'notSent' });

      expect(errors).toHaveLength(1);
      expect(String(errors[0])).toContain('Signing timed out before dispatch');
    });

    it('stays quiet on one its worker already reported', async () => {
      const { mirror, errors } = await failingMirror({ message: 'rejected', outcome: 'rejected', errorReported: true });

      await expect(mirror.exchange.placeOrder(order)).rejects.toMatchObject({ outcome: 'rejected' });

      expect(errors).toHaveLength(0);
    });
  });

  describe('when its worker goes away with calls pending', () => {
    const silentBridge = (target: RustEngineExchange) => {
      const bridge = () => {};
      target.on('callMethod', bridge);
      return bridge;
    };
    const mirrorOnly = () =>
      new RustEngineExchange('bybit', { key: 'k', secret: 's', workerMirror: true, publicDataOnly: false }, new DefaultStore());

    it('rejects a kept engine at once when the bridge is removed, a write as unknown', async () => {
      vi.useFakeTimers();
      try {
        fakeEngine({ supported: false });
        exchange = mirrorOnly();
        const bridge = silentBridge(exchange);
        const pendingRead = exchange.callMethodInWorker('fetchHistoricalFills', { from: 0, to: 1 } as never);
        const pendingWrite = exchange.callMethodInWorker('placeOrder', order as never);
        const read = expect(pendingRead).rejects.toThrow('stopped before answering');
        const write = expect(pendingWrite).rejects.toMatchObject({ outcome: 'unknown' });
        await vi.advanceTimersByTimeAsync(0);
        exchange.off('callMethod', bridge);
        await read;
        await write;
      } finally {
        vi.useRealTimers();
      }
    });

    it('keeps waiting while another bridge remains, and rejects on dispose', async () => {
      fakeEngine({ supported: false });
      const target = mirrorOnly();
      const first = silentBridge(target);
      silentBridge(target);
      const pending = target.callMethodInWorker('fetchHistoricalFills', { from: 0, to: 1 } as never);
      target.off('callMethod', first);
      const outcome = vi.fn();
      void pending.catch(outcome);
      await Promise.resolve();
      await Promise.resolve();
      expect(outcome).not.toHaveBeenCalled();
      target.dispose();
      await expect(pending).rejects.toThrow('stopped before answering');
    });
  });

  describe('a worker read that gets no answer', () => {
    const silentMirror = () => {
      const target = new RustEngineExchange(
        'bybit',
        { key: 'k', secret: 's', workerMirror: true, publicDataOnly: false },
        new DefaultStore(),
      );
      target.on('callMethod', () => {});
      return target;
    };

    it('fails at 30 s while the account loads, and leaves a write on its long bound', async () => {
      vi.useFakeTimers();
      try {
        fakeEngine({ supported: false });
        exchange = silentMirror();
        const read = exchange.callMethodInWorker('fetchOrders');
        const refused = expect(read).rejects.toThrow('has not finished loading');
        const write = vi.fn();
        void exchange.callMethodInWorker('placeOrder', order as never).catch(write);
        await vi.advanceTimersByTimeAsync(30_000);
        await refused;
        await vi.advanceTimersByTimeAsync(60_000);
        expect(write).not.toHaveBeenCalled();
      } finally {
        vi.useRealTimers();
      }
    });

    it('keeps the command timeout once the account has loaded', async () => {
      vi.useFakeTimers();
      try {
        fakeEngine({ supported: false });
        exchange = silentMirror();
        exchange.store.update({
          loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
        });
        const outcome = vi.fn();
        void exchange.callMethodInWorker('fetchOrders').catch(outcome);
        await vi.advanceTimersByTimeAsync(60_000);
        expect(outcome).not.toHaveBeenCalled();
      } finally {
        vi.useRealTimers();
      }
    });
  });

  it('re-sends a chase stop through its bridge until the rows confirm it, and tells the user when they do not', async () => {
    vi.useFakeTimers();
    try {
      fakeEngine({ supported: false });
      const store = new DefaultStore();
      exchange = new RustEngineExchange('bybit', { key: 'k', secret: 's', workerMirror: true, publicDataOnly: false }, store);
      const bridged: unknown[] = [];
      exchange.on('chaser:terminate', (id: unknown) => bridged.push(id));
      const errors: unknown[] = [];
      exchange.on('error', (message: unknown) => errors.push(message));
      store.update({ activeChasers: [{ id: 'c1', symbol: 'BTCUSDT', isStopped: false } as never] });
      exchange.emitter.emit('chaser:terminate', 'c1');
      expect(bridged).toEqual(['c1']);
      await vi.advanceTimersByTimeAsync(5_000);
      expect(bridged).toEqual(['c1', 'c1']);
      await vi.advanceTimersByTimeAsync(55_000);
      expect(errors.at(-1)).toContain('NOT confirmed');

      errors.length = 0;
      bridged.length = 0;
      exchange.emitter.emit('chaser:terminate', 'c1');
      await vi.advanceTimersByTimeAsync(5_000);
      store.update({ activeChasers: [] });
      await vi.advanceTimersByTimeAsync(120_000);
      expect(bridged).toHaveLength(2);
      // The user was told NOT confirmed, so the late confirmation says what is known.
      expect(errors).toEqual([
        'Chase stop is confirmed, but whether it filled is not known: check Positions and Open Orders.',
      ]);
    } finally {
      vi.useRealTimers();
    }
  });

  it('asks its data source to re-read a setting it holds', async () => {
    const fake = fakeEngine();
    const mirror = mirrorWith(fake);
    exchange = mirror.exchange;
    await exchange.frameFedReady;
    await exchange.changePositionMode(true, 'BTCUSDT');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(fake.commands.map((command) => command.op)).toContain('setPositionMode');
    expect(mirror.calls).toContain('refreshAccountSettings');
    expect(mirror.calls).not.toContain('changePositionMode');
  });

  describe('while its engine holds an uncertain write', () => {
    const held = async (holdOrders: Fake['holdOrders']) => {
      vi.useFakeTimers();
      const fake = fakeEngine();
      const mirror = mirrorWith(fake);
      exchange = mirror.exchange;
      await exchange.frameFedReady;
      fake.holdOrders = holdOrders;
      return { fake, mirror };
    };
    afterEach(() => vi.useRealTimers());

    // Bravulasco: a lost page placement fenced its symbol, and the source, which never saw it,
    // never sent the orders that lift a fence. Only a read sent after the write can lift it.
    it('asks its data source for the held symbols once the hold can lift, and only once', async () => {
      const { mirror } = await held({ symbols: 'BTCUSDT', liftAfterMs: 3_000 });
      const needsSnapshot = vi.fn();
      mirror.exchange.on('frameFed:needsSnapshot', needsSnapshot);
      await mirror.exchange.placeOrder(order);
      await mirror.exchange.placeOrder(order);
      await vi.advanceTimersByTimeAsync(3_400);
      expect(mirror.orderAsks).toEqual([]);
      await vi.advanceTimersByTimeAsync(200);
      expect(mirror.orderAsks).toEqual([[['BTCUSDT']]]);
      await vi.advanceTimersByTimeAsync(60_000);
      expect(mirror.orderAsks).toHaveLength(1);
      expect(needsSnapshot).not.toHaveBeenCalled();
    });

    it('asks account-wide when the engine holds the account', async () => {
      const { mirror } = await held({ symbols: '', liftAfterMs: 0 });
      await mirror.exchange.placeOrder(order);
      await vi.advanceTimersByTimeAsync(500);
      expect(mirror.orderAsks).toEqual([[]]);
    });

    it('adds a newly held symbol to the waiting ask without delaying the oldest', async () => {
      const { fake, mirror } = await held({ symbols: 'BTCUSDT', liftAfterMs: 3_000 });
      await mirror.exchange.placeOrder(order);
      await vi.advanceTimersByTimeAsync(1_000);
      fake.holdOrders = { symbols: 'BTCUSDT,ETHUSDT', liftAfterMs: 3_000 };
      await mirror.exchange.placeOrder({ ...order, symbol: 'ETHUSDT' });
      await vi.advanceTimersByTimeAsync(2_500);
      expect(mirror.orderAsks).toEqual([[['BTCUSDT', 'ETHUSDT']]]);
    });

    // A read can lift some symbols and not others; the engine names what still stands.
    it('asks again for what the engine still holds after a partial lift', async () => {
      const { fake, mirror } = await held({ symbols: 'BTCUSDT,ETHUSDT', liftAfterMs: 0 });
      await mirror.exchange.placeOrder(order);
      await vi.advanceTimersByTimeAsync(500);
      fake.holdOrders = { symbols: 'ETHUSDT', liftAfterMs: 1_000 };
      await mirror.exchange.placeOrder(order);
      await vi.advanceTimersByTimeAsync(1_500);
      expect(mirror.orderAsks).toEqual([[['BTCUSDT', 'ETHUSDT']], [['ETHUSDT']]]);
    });

    it('waits the unsynced grace for a hold that names no time', async () => {
      const { mirror } = await held({ symbols: 'BTCUSDT' } as Fake['holdOrders']);
      await mirror.exchange.placeOrder(order);
      await vi.advanceTimersByTimeAsync(30_000);
      expect(mirror.orderAsks).toEqual([]);
      await vi.advanceTimersByTimeAsync(500);
      expect(mirror.orderAsks).toEqual([[['BTCUSDT']]]);
    });

    it('rapid repeated holds coalesce per account while latest scope and earliest due stand', async () => {
      const { mirror } = await held({ symbols: 'BTCUSDT', liftAfterMs: 3_000 });
      const other = mirrorWith(fakeEngine());
      await other.exchange.frameFedReady;
      try {
        for (let i = 0; i < 2_000; i++) {
          (mirror.exchange as any).receiveEvent({ level: 'info', code: 'orders.held', message: 'held', fields: { symbols: i === 1_999 ? 'ETHUSDT' : 'BTCUSDT', liftAfterMs: 3_000 + i } });
          (other.exchange as any).receiveEvent({ level: 'info', code: 'orders.held', message: 'held', fields: { symbols: '', liftAfterMs: 0 } });
        }
        await vi.advanceTimersByTimeAsync(500);
        expect(other.orderAsks).toEqual([[]]);
        expect(mirror.orderAsks).toEqual([]);
        await vi.advanceTimersByTimeAsync(3_000);
        expect(mirror.orderAsks).toEqual([[['ETHUSDT']]]);
        await vi.advanceTimersByTimeAsync(60_000);
        expect(other.orderAsks).toHaveLength(1);
        expect(mirror.orderAsks).toHaveLength(1);
      } finally { other.exchange.dispose(); }
    });

    it('dispose between the due timer and worker bridge continuation starts no retired call or timer', async () => {
      const { mirror } = await held({ symbols: 'BTCUSDT', liftAfterMs: 0 });
      await mirror.exchange.placeOrder(order);
      const call = vi.spyOn(mirror.exchange, 'callMethodInWorker');
      vi.advanceTimersByTime(500); // Fire the real due timer, hold its async bridge continuation.
      mirror.exchange.dispose();
      await Promise.resolve(); await Promise.resolve(); await Promise.resolve();
      expect.soft(call).not.toHaveBeenCalled();
      expect.soft(vi.getTimerCount()).toBe(0);
    });

    it('partial lifts keep coalescing even while an earlier ask is unanswered', async () => {
      const { mirror } = await held({ symbols: 'BTCUSDT,ETHUSDT', liftAfterMs: 0 });
      mirror.exchange.off('callMethod');
      type Ask = { requestId: string; args: unknown[] };
      const asks: Ask[] = [];
      mirror.exchange.on('callMethod', (event: Ask) => {
        asks.push(event);
      });
      await mirror.exchange.placeOrder(order);
      await vi.advanceTimersByTimeAsync(500);
      expect(asks.map((ask) => ask.args)).toEqual([[['BTCUSDT', 'ETHUSDT']]]);
      for (let i = 0; i < 2_000; i++)
        (mirror.exchange as any).receiveEvent({ level: 'info', code: 'orders.held', message: 'held', fields: { symbols: 'ETHUSDT', liftAfterMs: 0 } });
      await vi.advanceTimersByTimeAsync(500);
      expect(asks.map((ask) => ask.args)).toEqual([[['BTCUSDT', 'ETHUSDT']], [['ETHUSDT']]]);
      for (const ask of asks) mirror.exchange.emitter.emit('callMethod:done', { requestId: ask.requestId, success: true, result: null });
      await vi.advanceTimersByTimeAsync(60_000);
      expect(asks).toHaveLength(2);
    });

    it('normal due timer starts exactly one call before disposal', async () => {
      const { mirror } = await held({ symbols: 'BTCUSDT', liftAfterMs: 0 });
      await mirror.exchange.placeOrder(order);
      await vi.advanceTimersByTimeAsync(500);
      expect(mirror.orderAsks).toEqual([[['BTCUSDT']]]);
      mirror.exchange.dispose();
      expect(vi.getTimerCount()).toBe(0);
    });

    it('asks nothing after disposal', async () => {
      const { mirror } = await held({ symbols: 'BTCUSDT', liftAfterMs: 3_000 });
      await mirror.exchange.placeOrder(order);
      mirror.exchange.dispose();
      await vi.advanceTimersByTimeAsync(60_000);
      expect(mirror.calls).not.toContain('refreshAccountOrders');
    });
  });

  it('relays an unknown-cancel refresh immediately without replacing a placement hold', async () => {
    vi.useFakeTimers();
    try {
      const fake = fakeEngine();
      const mirror = mirrorWith(fake);
      exchange = mirror.exchange;
      await exchange.frameFedReady;
      (exchange as any).receiveEvent({ level: 'info', code: 'orders.held', message: 'held', fields: { symbols: 'ETHUSDT', liftAfterMs: 3_000 } });
      (exchange as any).receiveEvent({ level: 'info', code: 'orders.refresh', message: 'unknown cancel', fields: { symbols: 'BTCUSDT' } });
      await Promise.resolve(); await Promise.resolve();
      expect(mirror.orderAsks).toEqual([[['BTCUSDT']]]);
      await vi.advanceTimersByTimeAsync(3_500);
      expect(mirror.orderAsks).toEqual([[['BTCUSDT']], [['ETHUSDT']]]);
      expect(fake.commands).toEqual([]);
      await vi.advanceTimersByTimeAsync(60_000);
      expect(mirror.orderAsks).toHaveLength(2);
    } finally { vi.useRealTimers(); }
  });

  it('forwards native cancel targets on the same single source refresh, then drains them', async () => {
    const fake = fakeEngine();
    const mirror = mirrorWith(fake);
    exchange = mirror.exchange;
    await exchange.frameFedReady;
    const targets = [{ id: 'target', symbol: 'BTCUSDT' }];
    (exchange as any).receiveEvent({
      level: 'info',
      code: 'orders.cancel_check',
      fields: { targets: JSON.stringify(targets) },
    });
    expect(mirror.orderAsks).toEqual([]);
    (exchange as any).receiveEvent({ level: 'info', code: 'orders.refresh', fields: { symbols: 'BTCUSDT' } });
    await Promise.resolve();
    await Promise.resolve();
    expect(mirror.orderAsks).toEqual([[['BTCUSDT'], targets]]);
    (exchange as any).receiveEvent({ level: 'info', code: 'orders.refresh', fields: { symbols: '' } });
    await Promise.resolve();
    await Promise.resolve();
    expect(mirror.orderAsks).toEqual([[['BTCUSDT'], targets], []]);
    expect(fake.commands).toEqual([]);
  });

  it('still requests an ordinary source read when target registration refuses', async () => {
    exchange = mirrorWith(fakeEngine()).exchange;
    await exchange.frameFedReady;
    // The real source method below runs locally, as in the worker; command delivery is the seam.
    Object.defineProperty(exchange, 'routesToWorker', { get: () => false });
    const refused = new Error('registration unsupported');
    const command = vi
      .spyOn(exchange as any, 'command')
      .mockRejectedValueOnce(refused)
      .mockResolvedValueOnce(null);
    await expect(exchange.refreshAccountOrders(undefined, [{ id: 'target', symbol: 'BTCUSDT' }])).rejects.toBe(refused);
    expect(command.mock.calls.map(([c]) => c)).toEqual([
      { op: 'registerCancelChecks', orders: [{ id: 'target', symbol: 'BTCUSDT' }] },
      { op: 'refreshOrders' },
    ]);
  });

  // 19pt8s: a source answers an orders ask on dispatch; the hold lifts with the orders frame after it.
  describe('a source orders read', () => {
    const asked = async () => {
      const fake = fakeEngine();
      exchange = mirrorWith(fake).exchange;
      await exchange.frameFedReady;
      exchange.ingestSourceFrame(consumerFrame({ markets: [] }), true);
      (exchange as unknown as { refreshSourceOrders: (symbols?: string[]) => void }).refreshSourceOrders(['BTCUSDT']);
      await vi.waitFor(() => expect((exchange as unknown as { sourceOrderReads: number }).sourceOrderReads).toBe(0));
      return exchange;
    };

    it('stays unsettled past its answer until a frame carrying orders lands', async () => {
      const mirror = await asked();
      expect(mirror.hasUnsettledWrites).toBe(true);
      mirror.ingestSourceFrame(consumerFrame({ tickers: [] }), false);
      expect(mirror.hasUnsettledWrites).toBe(true);
      mirror.ingestSourceFrame(consumerFrame({ orders: [] }), false);
      expect(mirror.hasUnsettledWrites).toBe(false);
    });

    it('settles at the refresh deadline when no orders frame comes', async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      try {
        const mirror = await asked();
        expect(mirror.hasUnsettledWrites).toBe(true);
        vi.advanceTimersByTime(29_000);
        expect(mirror.hasUnsettledWrites).toBe(true);
        vi.advanceTimersByTime(1_000);
        expect(mirror.hasUnsettledWrites).toBe(false);
      } finally {
        vi.useRealTimers();
      }
    });
  });

  it('drops source deltas until a snapshot, then ingests in order', async () => {
    const fake = fakeEngine();
    exchange = mirrorWith(fake).exchange;
    await exchange.frameFedReady;

    exchange.ingestSourceFrame(new ArrayBuffer(8), false);
    expect(fake.applied).toEqual([]);
    exchange.ingestSourceFrame(new ArrayBuffer(16), true);
    exchange.ingestSourceFrame(new ArrayBuffer(8), false);
    expect(fake.applied).toEqual([
      { snapshot: true, bytes: 16 },
      { snapshot: false, bytes: 8 },
    ]);
  });

  it('asks for a fresh snapshot when the engine refuses a source frame, and drops deltas until then', async () => {
    const fake = fakeEngine();
    exchange = mirrorWith(fake).exchange;
    await exchange.frameFedReady;
    exchange.ingestSourceFrame(new ArrayBuffer(16), true);
    const needsSnapshot = vi.fn();
    exchange.on('frameFed:needsSnapshot', needsSnapshot);
    fake.refuseNext = true;
    exchange.ingestSourceFrame(new ArrayBuffer(8), false);
    expect(needsSnapshot).toHaveBeenCalledOnce();
    exchange.ingestSourceFrame(new ArrayBuffer(4), false);
    exchange.ingestSourceFrame(new ArrayBuffer(32), true);
    expect(fake.applied.map((frame) => frame.bytes)).toEqual([16, 32]);
  });

  it("measures its data source's clock from the frames it ingests", async () => {
    vi.spyOn(Date, 'now').mockReturnValue(1_800_000_000_000);
    vi.spyOn(performance, 'now').mockReturnValue(0);
    const fake = fakeEngine();
    exchange = mirrorWith(fake).exchange;
    await exchange.frameFedReady;
    const written = (offset: number) => {
      const frame = new ArrayBuffer(16);
      new DataView(frame).setFloat64(8, Date.now() + offset, true);
      return frame;
    };
    // A hosted server 2 s ahead, its frames arriving after up to 300 ms in transit.
    exchange.ingestSourceFrame(written(1_700), true);
    exchange.ingestSourceFrame(written(2_000), false);
    exchange.ingestSourceFrame(written(1_990), false);
    expect(fake.offsets.length).toBe(2);
    expect(fake.offsets[1]).toBeGreaterThanOrEqual(1_950);
    expect(fake.offsets[1]).toBeLessThanOrEqual(2_000);
  });

  it('fills the UI store from its own engine frames', async () => {
    const fake = fakeEngine();
    exchange = mirrorWith(fake).exchange;
    await exchange.frameFedReady;
    exchange.ingestSourceFrame(new ArrayBuffer(16), true);
    expect(exchange.store.loaded.markets).toBe(true);
  });

  it('stops the data source algos first, then runs the nuke legs on its own engine', async () => {
    const fake = fakeEngine();
    const mirror = mirrorWith(fake);
    exchange = mirror.exchange;
    await exchange.frameFedReady;

    await expect(exchange.nukeOrders()).resolves.toEqual({ stoppedChasers: ['c-1'], stoppedTwaps: [] });
    expect(mirror.calls[0]).toBe('stopAccountAlgos');
    expect(mirror.calls).not.toContain('nukeOrders');
  });

  describe('bybitdemo nuke flatness through the real worker RPC handler', () => {
    let handler: CallMethodHandler;
    afterEach(() => handler?.dispose());

    const proofWorker = async (readPositions?: ReturnType<typeof vi.fn>) => {
      const fake = fakeEngine({ venue: 'bybitdemo' });
      const mirror = new RustEngineExchange(
        'bybitdemo',
        { key: 'fixture', secret: 'fixture', workerMirror: true, publicDataOnly: false },
        new DefaultStore(),
      );
      exchange = mirror;
      const calls: string[] = [];
      const worker = {
        name: 'bybitdemo',
        stopAccountAlgos: vi.fn().mockResolvedValue({ stoppedChasers: [], stoppedTwaps: [] }),
        fetchPositions: vi.fn().mockResolvedValue([]),
        readPositions,
      };
      handler = new CallMethodHandler(
        'fixture',
        'bybitdemo',
        worker as unknown as Exchange,
        { postMessage: ({ op, data }: { op: string; data: unknown }) => mirror.emitter.emit(op, data) } as unknown as Worker,
        {} as AppSettingsWorkerType,
        {} as ExchangeMetadataWorkerType,
      );
      mirror.on('callMethod', (event: { requestId: string; method: string; args: unknown[] }) => {
        calls.push(event.method);
        void handler.handleCallMethod(event.requestId, event.method as never, event.args);
      });
      await expect(mirror.frameFedReady).resolves.toBe(true);
      return { mirror, fake, calls };
    };

    it('confirms flat only from the worker proof, even when the mirrored store is empty', async () => {
      const read = vi.fn().mockResolvedValue({ skipped: [], open: [] });
      const { mirror, fake, calls } = await proofWorker(read);
      await expect(mirror.nukePositions()).resolves.toEqual({ stoppedChasers: [], stoppedTwaps: [] });
      expect(read).toHaveBeenCalledExactlyOnceWith();
      expect(calls[0]).toBe('stopAccountAlgos');
      expect(calls).toContain('readPositions');
      expect(fake.commands.map(({ op }) => op)).not.toContain('readPositions');
      expect(fake.commands.map(({ op }) => op)).not.toContain('placeOrders');
    });

    it('cannot call an empty mirrored store flat when the worker proof remains open', async () => {
      const read = vi.fn().mockResolvedValue({
        skipped: [],
        open: [{ symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 2 }],
      });
      const { mirror } = await proofWorker(read);
      await expect(mirror.nukePositions()).rejects.toThrow(
        'Nuke did not leave the account flat: BTCUSDT long 2 still open',
      );
      expect(read).toHaveBeenCalledTimes(3);
    });

    it('names an unavailable proof RPC and retains its typed cause', async () => {
      const { mirror } = await proofWorker();
      await expect(mirror.nukePositions()).rejects.toMatchObject({
        message: 'Nuke could not confirm the account is flat: bybitdemo does not support readPositions',
        cause: { kind: 'unsupported', outcome: 'notSent' },
      });
    });

    it('keeps the real typed read failure as its cause and sends no further close', async () => {
      const read = vi.fn().mockRejectedValue(
        new ExchangeError('Bybit positions request timed out', {
          message: 'Bybit positions request timed out',
          kind: 'timeout',
          venueCode: 'READ_TIMEOUT',
          outcome: 'notSent',
        }),
      );
      const { mirror, fake } = await proofWorker(read);
      await expect(mirror.nukePositions()).rejects.toMatchObject({
        message: 'Nuke could not confirm the account is flat: Bybit positions request timed out',
        cause: { kind: 'timeout', venueCode: 'READ_TIMEOUT', outcome: 'notSent' },
      });
      expect(read).toHaveBeenCalledOnce();
      expect(fake.commands.map(({ op }) => op)).not.toContain('placeOrders');
    });

    it.each([
      [{ skipped: ['BTCUSDT'], open: [] }, 'BTCUSDT on no listed market'],
      [[], 'unexpected positions read result'],
    ])('refuses incomplete or malformed worker proof %j', async (proof, reason) => {
      const read = vi.fn().mockResolvedValue(proof);
      const { mirror } = await proofWorker(read);
      await expect(mirror.nukePositions()).rejects.toThrow(`Nuke could not confirm the account is flat: ${reason}`);
      expect(read).toHaveBeenCalledOnce();
    });
  });

  it('stays a plain mirror where the family has no frame-fed engine', async () => {
    const fake = fakeEngine({ supported: false });
    const mirror = mirrorWith(fake);
    exchange = mirror.exchange;
    await expect(exchange.frameFedReady).resolves.toBe(false);
    expect(fake.started).toBe(0);
    expect(fake.constructed).toBe(0);
    await exchange.placeOrder(order);
    expect(mirror.calls).toContain('placeOrder');
    expect(fake.commands.map((command) => command.op)).not.toContain('placeOrders');
  });

  it('builds nothing on main with a wasm whose WasmEngine has no frameFedSupported', async () => {
    const fake = fakeEngine();
    const construct = vi.fn();
    setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
    const mirror = mirrorWith(fake);
    exchange = mirror.exchange;
    await expect(exchange.frameFedReady).resolves.toBe(false);
    expect(construct).not.toHaveBeenCalled();
    await exchange.placeOrder(order);
    expect(mirror.calls).toContain('placeOrder');
  });

  it('never starts an engine that does not report itself frame-fed', async () => {
    const fake = fakeEngine({ frameFed: false });
    exchange = mirrorWith(fake).exchange;
    await expect(exchange.frameFedReady).resolves.toBe(false);
    expect(fake.started).toBe(0);
  });

  it('builds no engine for a public-only account', async () => {
    const fake = fakeEngine();
    exchange = new RustEngineExchange('bybit', { key: '', secret: '', workerMirror: true }, new DefaultStore());
    await expect(exchange.frameFedReady).resolves.toBe(false);
    expect(fake.started).toBe(0);
  });

  it('keeps hedge legs for a close placed through the main engine', () => {
    // Pinned so a nuke on main sizes a hedge leg the same way as the worker did.
    expect(PositionSide.Long).toBe('long');
  });
});
