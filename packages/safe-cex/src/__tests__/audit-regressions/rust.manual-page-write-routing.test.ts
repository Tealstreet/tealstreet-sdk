import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { consumerFrame } from '../cutover/consumerFrame';
import { engineCapabilities } from '../fixtures/engineCapabilities';

const order = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 2 };

type Fake = {
  commands: Array<{ op: string; [field: string]: unknown }>;
  applied: Array<{ snapshot: boolean; bytes: number }>;
  started: number;
  constructed: number;
  refuseNext?: boolean;
  commandError?: string;
  callerReports?: boolean;
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
      if (command.op === 'placeOrders' && fake.commandError)
        return JSON.stringify([{ op: 'done', command: command.command, error: fake.commandError }]);
      const done = { op: 'done', command: command.command, result: JSON.stringify(result) };
      // A confirmed settings write holds it over the source, as a frame-fed venue does.
      const held = {
        op: 'event',
        level: 'info',
        code: 'settings.held',
        message: 'held',
        fields: { symbol: 'BTCUSDT' },
      };
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

const mirrorWith = (
  fake: Fake,
  failure?: Record<string, unknown>,
  pause?: (event: { requestId: string; method: string }, exchange: RustEngineExchange) => boolean,
) => {
  void fake;
  const exchange = new RustEngineExchange(
    'bybit',
    {
      key: 'k',
      secret: 's',
      workerMirror: true,
      publicDataOnly: false,
      callerReportsPlacementFailures: fake.callerReports,
    },
    new DefaultStore(),
  );
  const calls: string[] = [];
  const orderAsks: unknown[][] = [];
  exchange.on('callMethod', (event: { requestId: string; method: string; args: unknown[] }) => {
    calls.push(event.method);
    if (pause?.(event, exchange)) return;
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

const flush = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
};
const manualMethods = [
  'placeOrder',
  'placeOrders',
  'placeOrderWithClientOrderId',
  'updateOrder',
  'cancelOrders',
  'cancelSymbolOrders',
  'cancelAllOrders',
  'nuke',
  'nukePositions',
  'nukeOrders',
  'setLeverage',
  'setAllLeverage',
  'changeMarginType',
  'changePositionMode',
  'changeStpMode',
  'changeAccountTier',
  'scheduleCancel',
] as const;

describe('manual page writes never fall back to the hosted RPC', () => {
  let exchange: RustEngineExchange | undefined;
  afterEach(() => {
    exchange?.dispose();
    exchange = undefined;
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it.each(['direct', 'worker-helper'])('waits for page readiness for leverage through %s', async (entry) => {
    const fake = fakeEngine();
    const module = lastModule;
    if (!module) throw new Error('Missing synthetic module');
    let release = () => {};
    const startup = new Promise<WasmEngineModule>((resolve) => {
      release = () => resolve(module);
    });
    setWasmEngineLoader(() => startup);
    const mirror = mirrorWith(fake);
    exchange = mirror.exchange;
    let settled = false;
    const pending = (
      entry === 'direct' ? exchange.setLeverage('BTCUSDT', 5) : exchange.callMethodInWorker('setLeverage', 'BTCUSDT', 5)
    ).finally(() => {
      settled = true;
    });
    void pending.catch(() => {});
    await flush();
    try {
      expect(mirror.calls).toEqual([]);
      expect(settled).toBe(false);
    } finally {
      release();
    }
    await pending;
    expect(fake.commands.filter(({ op }) => op === 'setLeverage')).toHaveLength(1);
    expect(mirror.calls).toEqual([]);
  });

  it.each(manualMethods)('refuses unsupported page engine before any %s RPC', async (method) => {
    const fake = fakeEngine({ supported: false });
    const mirror = mirrorWith(fake);
    exchange = mirror.exchange;
    await expect(exchange.frameFedReady).resolves.toBe(false);
    const errors = vi.fn();
    exchange.on('error', errors);
    await expect(Reflect.apply(exchange.callMethodInWorker, exchange, [method])).rejects.toMatchObject({
      outcome: 'notSent',
    });
    expect(errors).toHaveBeenCalledOnce();
    expect(mirror.calls).toEqual([]);
    expect(fake.commands).toEqual([]);
  });

  it('ready leverage uses the page command while leverage reads use hosted RPC', async () => {
    const fake = fakeEngine();
    const mirror = mirrorWith(fake);
    exchange = mirror.exchange;
    await expect(exchange.frameFedReady).resolves.toBe(true);
    await exchange.setLeverage('BTCUSDT', 5);
    await exchange.fetchLeverageForSymbol('BTCUSDT');
    expect(fake.commands.filter(({ op }) => op === 'setLeverage')).toEqual([
      expect.objectContaining({ symbol: 'BTCUSDT', leverage: 5 }),
    ]);
    expect(mirror.calls).toEqual(['fetchLeverageForSymbol']);
  });

  it('times out locally and never sends after late readiness', async () => {
    vi.useFakeTimers();
    const fake = fakeEngine();
    const module = lastModule;
    if (!module) throw new Error('Missing synthetic module');
    let release = () => {};
    const startup = new Promise<WasmEngineModule>((resolve) => {
      release = () => resolve(module);
    });
    setWasmEngineLoader(() => startup);
    const mirror = mirrorWith(fake);
    exchange = mirror.exchange;
    const result = exchange.setLeverage('BTCUSDT', 5).then(
      () => undefined,
      (error: unknown) => error,
    );
    await vi.advanceTimersByTimeAsync(30_000);
    expect(await result).toMatchObject({ outcome: 'notSent' });
    release();
    await exchange.frameFedReady;
    expect(mirror.calls).toEqual([]);
    expect(fake.commands.filter(({ op }) => op === 'setLeverage')).toEqual([]);
  });

  it.each(['owner-switch', 'owner-round-trip', 'disposed'])('drops the waiting writer on %s', async (transition) => {
    const fake = fakeEngine();
    const module = lastModule;
    if (!module) throw new Error('Missing synthetic module');
    let release = () => {};
    const startup = new Promise<WasmEngineModule>((resolve) => {
      release = () => resolve(module);
    });
    setWasmEngineLoader(() => startup);
    const mirror = mirrorWith(fake);
    exchange = mirror.exchange;
    let generation = 1;
    let ownerId = 'fake-owner-A';
    exchange.setWorkerOwner(() => ({ ownerId, generation }));
    const result = exchange.setLeverage('BTCUSDT', 5).then(
      () => undefined,
      (error: unknown) => error,
    );
    await flush();
    if (transition === 'disposed') exchange.dispose();
    else {
      ownerId = 'fake-owner-B';
      generation++;
      if (transition === 'owner-round-trip') {
        ownerId = 'fake-owner-A';
        generation++;
      }
    }
    release();
    await exchange.frameFedReady;
    expect(await result).toMatchObject({ outcome: 'notSent' });
    expect(mirror.calls).toEqual([]);
    expect(fake.commands.filter(({ op }) => op === 'setLeverage')).toEqual([]);
  });

  describe.each(['ready', 'startup-wait'])('composite admission: %s', (readiness) => {
    it.each(['setAllLeverage', 'cancelAllOrders', 'nukeOrders', 'nukePositions', 'nuke'] as const)(
      'retains the original owner across %s internal reads',
      async (method) => {
        const fake = fakeEngine();
        const module = lastModule;
        if (!module) throw new Error('Missing synthetic module');
        let release = () => {};
        const startup = new Promise<WasmEngineModule>((resolve) => {
          release = () => resolve(module);
        });
        if (readiness === 'startup-wait') setWasmEngineLoader(() => startup);
        let reached = () => {};
        const atRead = new Promise<void>((resolve) => {
          reached = resolve;
        });
        let reply = () => {};
        const paused =
          method === 'setAllLeverage'
            ? 'fetchPositions'
            : method === 'cancelAllOrders'
              ? 'fetchOrders'
              : 'stopAccountAlgos';
        const mirror = mirrorWith(fake, undefined, (event, current) => {
          if (event.method !== paused) return false;
          reply = () =>
            current.emitter.emit('callMethod:done', {
              requestId: event.requestId,
              success: true,
              result:
                paused === 'stopAccountAlgos'
                  ? { stoppedChasers: [], stoppedTwaps: [], placedSymbols: ['BTCUSDT'] }
                  : [{ symbol: 'BTCUSDT', side: 'long' }],
            });
          reached();
          return true;
        });
        exchange = mirror.exchange;
        let generation = 1;
        exchange.setWorkerOwner(() => ({ ownerId: 'fake-owner-A', generation }));
        if (readiness === 'ready') await exchange.frameFedReady;
        const pending = (method === 'setAllLeverage' ? exchange.setAllLeverage(5) : exchange[method]()).then(
          () => undefined,
          (error: unknown) => error,
        );
        release();
        await atRead;
        generation += 2;
        reply();
        expect(await pending).toMatchObject({ outcome: 'notSent' });
        expect(
          fake.commands.filter(({ op }) =>
            ['setLeverage', 'cancelSymbolOrders', 'cancelAccountOrders', 'placeOrders'].includes(op),
          ),
        ).toEqual([]);
      },
    );
  });

  it('keeps the same-owner bulk leverage command payloads and order', async () => {
    const fake = fakeEngine();
    const mirror = mirrorWith(fake, undefined, (event, current) => {
      if (event.method !== 'fetchPositions') return false;
      current.emitter.emit('callMethod:done', {
        requestId: event.requestId,
        success: true,
        result: [
          { symbol: 'BTCUSDT', side: 'long' },
          { symbol: 'ETHUSDT', side: 'short' },
        ],
      });
      return true;
    });
    exchange = mirror.exchange;
    exchange.setWorkerOwner(() => ({ ownerId: 'fake-owner-A', generation: 1 }));
    await exchange.frameFedReady;
    await exchange.setAllLeverage(5);
    expect(
      fake.commands.filter(({ op }) => op === 'setLeverage').map(({ command: _command, ...payload }) => payload),
    ).toEqual([
      { op: 'setLeverage', symbol: 'BTCUSDT', leverage: 5, side: 'long', timeoutMs: 120000 },
      { op: 'setLeverage', symbol: 'ETHUSDT', leverage: 5, side: 'short', timeoutMs: 120000 },
    ]);
  });

  it('reports a direct unsupported scheduleCancel exactly once', async () => {
    const fake = fakeEngine({ supported: false });
    const mirror = mirrorWith(fake);
    exchange = mirror.exchange;
    await exchange.frameFedReady;
    const errors = vi.fn();
    exchange.on('error', errors);
    await expect(exchange.scheduleCancel()).rejects.toMatchObject({ outcome: 'notSent' });
    expect(errors).toHaveBeenCalledOnce();
    expect(mirror.calls).toEqual([]);
  });

  it('reports an unsupported capability-adopted account tier exactly once', async () => {
    const fake = fakeEngine({ supported: false, venue: 'lighter' });
    exchange = new RustEngineExchange(
      'lighter',
      { key: 'fake-key', secret: 'fake-secret', workerMirror: true },
      new DefaultStore(),
    );
    exchange.on('callMethod', (event: { requestId: string; method: string }) => {
      exchange?.emitter.emit('callMethod:done', {
        requestId: event.requestId,
        success: true,
        result: JSON.parse(engineCapabilities('lighter')),
      });
    });
    await exchange.frameFedReady;
    await exchange.loadCapabilities();
    const errors = vi.fn();
    exchange.on('error', errors);
    await expect(exchange.changeAccountTier?.('premium')).rejects.toMatchObject({ outcome: 'notSent' });
    expect(errors).toHaveBeenCalledOnce();
    expect(fake.commands).toEqual([]);
  });

  it.each([
    [true, 0],
    [false, 1],
  ] as const)('keeps ready-page placement failure reporting with its caller=%s', async (owned, reports) => {
    const fake = fakeEngine();
    fake.commandError = 'Synthetic venue refused the placement';
    fake.callerReports = owned;
    exchange = mirrorWith(fake).exchange;
    await exchange.frameFedReady;
    const errors = vi.fn();
    exchange.on('error', errors);
    await expect(exchange.placeOrders([order])).rejects.toThrow(fake.commandError);
    expect(errors).toHaveBeenCalledTimes(reports);
  });
});
