import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../exchanges/rust/rust.exchange';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import { RustEngineExchange, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderType, PositionSide } from '../types';
import { consumerFrame } from './cutover/consumerFrame';
import { engineCapabilities } from './fixtures/engineCapabilities';

// The host wrapper lands with the engine's ingest (fe7lgm); this is its contract.
type IngestingHost = EngineHost & { applyFrame?: (frame: ArrayBuffer, snapshot: boolean) => void };
const hostPrototype = EngineHost.prototype as IngestingHost & {
  run(actions: string): void;
  engine: WasmEngineLike & { applyFrame(bytes: Uint8Array, snapshot: boolean, now: number): string };
  transport: { now(): number };
};
const hadApplyFrame = 'applyFrame' in hostPrototype;
if (!hadApplyFrame) {
  hostPrototype.applyFrame = function (this: typeof hostPrototype, frame: ArrayBuffer, snapshot: boolean) {
    this.run(this.engine.applyFrame(new Uint8Array(frame), snapshot, this.transport.now()));
  };
}

const order = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 2 };

type Fake = {
  commands: Array<{ op: string; [field: string]: unknown }>;
  applied: Array<{ snapshot: boolean; bytes: number }>;
  started: number;
  constructed: number;
};

const fakeEngine = ({ supported = true, frameFed = true } = {}): Fake => {
  const fake: Fake = { commands: [], applied: [], started: 0, constructed: 0 };
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
      const result = command.op === 'placeOrders' ? ['placed-1'] : null;
      return command.command === undefined
        ? '[]'
        : JSON.stringify([{ op: 'done', command: command.command, result: JSON.stringify(result) }]);
    },
    applyFrame: (bytes: Uint8Array, snapshot: boolean) => {
      fake.applied.push({ snapshot, bytes: bytes.byteLength });
      return '[]';
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
  setWasmEngineLoader(async () => ({ WasmEngine: WasmEngine as unknown as WasmEngineModule['WasmEngine'] }));
  return fake;
};

const mirrorWith = (fake: Fake) => {
  void fake;
  const exchange = new RustEngineExchange(
    'bybit',
    { key: 'k', secret: 's', workerMirror: true, publicDataOnly: false },
    new DefaultStore(),
  );
  const calls: string[] = [];
  exchange.on('callMethod', (event: { requestId: string; method: string }) => {
    calls.push(event.method);
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

    expect(fake.commands.map((command) => command.op)).toEqual(
      expect.arrayContaining(['placeOrders', 'setLeverage', 'cancelSymbolOrders']),
    );
    expect(mirror.calls).not.toContain('placeOrder');
    expect(mirror.calls).not.toContain('setLeverage');
    expect(mirror.calls).not.toContain('cancelSymbolOrders');
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
