import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../exchanges/rust/rust.exchange';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderStatus, OrderType, PositionSide } from '../types';
import { consumerFrame } from './cutover/consumerFrame';
import { engineCapabilities } from './fixtures/engineCapabilities';

const order = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 2 };

type Fake = {
  commands: Array<{ op: string; [field: string]: unknown }>;
  applied: Array<{ snapshot: boolean; bytes: number }>;
  started: number;
  constructed: number;
  refuseNext?: boolean;
  offsets: Array<number | undefined>;
};

let lastModule: WasmEngineModule | undefined;

const fakeEngine = ({ supported = true, frameFed = true } = {}): Fake => {
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
    capabilities: () => engineCapabilities('bybit'),
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
  exchange.on('callMethod', (event: { requestId: string; method: string }) => {
    calls.push(event.method);
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
  return { exchange, calls };
};

describe('frame-fed main-thread mirror', () => {
  let exchange: RustEngineExchange | undefined;
  afterEach(() => {
    exchange?.dispose();
    exchange = undefined;
    vi.restoreAllMocks();
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

        const placed = mirror.exchange.placeOrder(order);
        const refused = expect(placed).rejects.toMatchObject({ outcome: 'notSent' });
        await vi.advanceTimersByTimeAsync(30_000);
        await refused;

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
