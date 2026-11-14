import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions, Order } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';
import { consumerFrame } from '../cutover/consumerFrame';
import { engineCapabilities } from '../fixtures/engineCapabilities';

const placed: Order = {
  id: 'existing', symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy,
  price: 100, amount: 2, filled: 0, remaining: 2, reduceOnly: false,
  status: OrderStatus.Open, lastUpdate: 1,
};

const fakeModule = (venue: ExchangeName, supported: boolean | undefined, constructionFrameFed = true) => {
  const commands: string[] = [];
  const constructed = vi.fn();
  const idle = () => '[]';
  const frame = () => new Uint8Array(consumerFrame({
    markets: [], tickers: [],
    loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
  }));
  const engine: WasmEngineLike = {
    syncHostTime: () => true,
    capabilities: () => engineCapabilities(venue),
    construction: () => JSON.stringify({ frameFed: constructionFrameFed, canSign: true, publicDataOnly: false }),
    start: idle, wsOpen: idle, wsMessage: idle, wsClose: idle, wsRouteExhausted: idle,
    httpStart: idle, httpChunk: idle, httpEnd: idle, httpFailed: idle, timer: idle,
    httpCommandOwner: () => undefined,
    applyFrame: idle, snapshotFrame: frame, takeFrame: frame,
    command: (json) => {
      const command: { op: string; command?: number } = JSON.parse(json);
      commands.push(command.op);
      return command.command === undefined ? '[]' : JSON.stringify([{
        op: 'done', command: command.command,
        result: JSON.stringify(['placeOrders', 'amendOrder', 'amendOrderWithSymbol'].includes(command.op) ? ['native'] : null),
      }]);
    },
  };
  const construct = function () { constructed(); return engine; };
  const module: WasmEngineModule = {
    WasmEngine: Object.assign(construct, supported === undefined ? {} : { frameFedSupported: () => supported }) as
      unknown as WasmEngineModule['WasmEngine'],
  };
  return { module, commands, constructed };
};

describe('COSIGNED manual writes without a frame-fed implementation', () => {
  const exchanges: RustEngineExchange[] = [];
  afterEach(() => {
    exchanges.forEach((exchange) => exchange.dispose());
    exchanges.length = 0;
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  const mirror = (venue: ExchangeName, options: Partial<ExchangeOptions> = {}) => {
    const exchange = new RustEngineExchange(venue, {
      key: 'synthetic-key', secret: '11'.repeat(32), workerMirror: true,
      signingMode: 'cosigned', publicDataOnly: false, ...options,
    }, new DefaultStore());
    exchanges.push(exchange);
    exchange.setWorkerOwner(() => ({ ownerId: 'synthetic-owner', generation: 1 }));
    const calls: Array<{ method: string; args: unknown[] }> = [];
    const bridge = (event: { requestId: string; method: string; args: unknown[] }) => {
      calls.push({ method: event.method, args: event.args });
      const result = event.method === 'updateOrder' ? { orderIds: ['hosted'], clientOrderId: 'replacement' }
        : event.method.startsWith('place') ? ['hosted'] : null;
      exchange.emitter.emit('callMethod:done', { requestId: event.requestId, success: true, result });
    };
    exchange.on('callMethod', bridge);
    return { exchange, calls, bridge };
  };

  for (const venue of ['grvt', 'mexc'] as const) {
    for (const method of ['placeOrder', 'updateOrder', 'cancelOrders', 'stopLoss', 'takeProfit', 'callMethodInWorker'] as const) {
      it(`${venue} sends ${method} once through the existing hosted method`, async () => {
        const fake = fakeModule(venue, false);
        setWasmEngineLoader(async () => fake.module);
        const { exchange, calls } = mirror(venue);
        const protective = { ...placed, reduceOnly: true,
          type: method === 'stopLoss' ? OrderType.StopLoss : OrderType.TakeProfit };
        const args = method === 'updateOrder' ? { order: placed, update: { price: 101 } }
          : { order: protective, update: { price: 101 } };
        if (method === 'placeOrder') await expect(exchange.placeOrder(placed)).resolves.toEqual(['hosted']);
        else if (method === 'cancelOrders') await exchange.cancelOrders([placed]);
        else if (method === 'callMethodInWorker') await expect(exchange.callMethodInWorker('placeOrder', placed)).resolves.toEqual(['hosted']);
        else {
          const ids = await exchange.updateOrder(args);
          expect(Array.from(ids)).toEqual(['hosted']);
          expect(ids).toHaveProperty('clientOrderId', 'replacement');
        }
        const wireMethod = ['stopLoss', 'takeProfit'].includes(method) ? 'updateOrder'
          : method === 'callMethodInWorker' ? 'placeOrder' : method;
        const wireArgs = method === 'cancelOrders' ? [[placed]]
          : wireMethod === 'updateOrder' ? [args] : [placed];
        expect(calls).toEqual([{ method: wireMethod, args: wireArgs }]);
        expect(fake.constructed).not.toHaveBeenCalled();
        expect(fake.commands).toEqual([]);
      });
    }
  }

  it('keeps Bybit manual place/amend/cancel on the page, never on the hosted RPC', async () => {
    const fake = fakeModule('bybit', true);
    setWasmEngineLoader(async () => fake.module);
    const { exchange, calls } = mirror('bybit');
    await expect(exchange.frameFedReady).resolves.toBe(true);
    await expect(exchange.placeOrder(placed)).resolves.toEqual(['native']);
    await exchange.updateOrder({ order: placed, update: { price: 101 } });
    await exchange.cancelOrders([placed]);
    await exchange.callMethodInWorker('placeOrder', placed);
    expect(fake.commands).toEqual(['placeOrders', 'amendOrder', 'cancelOrders', 'placeOrders']);
    expect(calls).toEqual([]);
  });

  it.each(['local', 'delegated', undefined] as const)('does not infer COSIGNED from a mirror (mode=%s)', async (signingMode) => {
    const fake = fakeModule('mexc', false);
    setWasmEngineLoader(async () => fake.module);
    const { exchange, calls } = mirror('mexc', { signingMode });
    await expect(exchange.placeOrder(placed)).rejects.toMatchObject({ outcome: 'notSent' });
    expect(calls).toEqual([]);
  });

  it('uses the current mode when the host keeps a mirror across LOCAL/COSIGNED switches', async () => {
    const fake = fakeModule('mexc', false);
    setWasmEngineLoader(async () => fake.module);
    const { exchange, calls } = mirror('mexc', { signingMode: 'local' });
    await expect(exchange.placeOrder(placed)).rejects.toMatchObject({ outcome: 'notSent' });
    exchange.options = { ...exchange.options, signingMode: 'cosigned' };
    await expect(exchange.placeOrder(placed)).resolves.toEqual(['hosted']);
    exchange.options = { ...exchange.options, signingMode: 'local' };
    await expect(exchange.placeOrder(placed)).rejects.toMatchObject({ outcome: 'notSent' });
    expect(calls).toEqual([{ method: 'placeOrder', args: [placed] }]);
    expect(fake.constructed).not.toHaveBeenCalled();
  });

  it.each(['missing capability', 'failed supported construction', 'failed module load'])('never uses hosted RPC after %s', async (failure) => {
    const fake = fakeModule('bybit', failure === 'missing capability' ? undefined : true, false);
    setWasmEngineLoader(async () => { if (failure === 'failed module load') throw new Error('synthetic module failure'); return fake.module; });
    const { exchange, calls } = mirror('bybit');
    await expect(exchange.placeOrder(placed)).rejects.toMatchObject({ outcome: 'notSent' });
    expect(calls).toEqual([]);
  });

  it('gives actionable text for a structurally unavailable writer', async () => {
    const fake = fakeModule('mexc', false);
    setWasmEngineLoader(async () => fake.module);
    const { exchange } = mirror('mexc', { signingMode: 'local' });
    await expect(exchange.placeOrder(placed)).rejects.toThrow('Turn cloud off to use this device');
  });

  it('retains the initiating owner while the capability module loads', async () => {
    const fake = fakeModule('mexc', false);
    let release: (module: WasmEngineModule) => void = () => { throw new Error('uninitialized'); };
    setWasmEngineLoader(() => new Promise((resolve) => { release = resolve; }));
    const { exchange, calls } = mirror('mexc');
    let generation = 1;
    exchange.setWorkerOwner(() => ({ ownerId: 'synthetic-owner', generation }));
    const pending = exchange.placeOrder(placed);
    const refused = expect(pending).rejects.toMatchObject({ outcome: 'notSent', code: 'WORKER_STOPPED' });
    generation = 2;
    release(fake.module);
    await refused;
    expect(calls).toEqual([]);
  });

  it.each(['owner', 'bridge', 'dispose'] as const)('does not dispatch a queued hosted write after %s retirement', async (retirement) => {
    const fake = fakeModule('grvt', false);
    let release: (module: WasmEngineModule) => void = () => { throw new Error('uninitialized'); };
    setWasmEngineLoader(() => new Promise((resolve) => { release = resolve; }));
    const { exchange, calls, bridge } = mirror('grvt');
    const pending = exchange.callMethodInWorker('placeOrder', placed);
    const refused = expect(pending).rejects.toMatchObject({ outcome: 'notSent', code: 'WORKER_STOPPED' });
    if (retirement === 'owner') exchange.setWorkerOwner(() => undefined);
    else if (retirement === 'bridge') { exchange.off('callMethod', bridge); exchange.on('callMethod', bridge); }
    else exchange.dispose();
    release(fake.module);
    await refused;
    expect(calls).toEqual([]);
  });

  it('does not dispatch after the startup deadline, even when support resolves later', async () => {
    vi.useFakeTimers();
    const fake = fakeModule('mexc', false);
    let release: (module: WasmEngineModule) => void = () => { throw new Error('uninitialized'); };
    setWasmEngineLoader(() => new Promise((resolve) => { release = resolve; }));
    const { exchange, calls } = mirror('mexc', { extra: { rustCommandTimeoutMs: 20 } });
    const pending = exchange.placeOrder(placed);
    const refused = expect(pending).rejects.toMatchObject({ outcome: 'notSent', kind: 'timeout' });
    await vi.advanceTimersByTimeAsync(30);
    await refused;
    release(fake.module);
    await vi.advanceTimersByTimeAsync(50);
    expect(calls).toEqual([]);
  });

  it('preserves an Unknown hosted result without re-sending or starting a page writer', async () => {
    const fake = fakeModule('mexc', false);
    setWasmEngineLoader(async () => fake.module);
    const { exchange, calls, bridge } = mirror('mexc');
    exchange.off('callMethod', bridge);
    exchange.on('callMethod', (event: { requestId: string; method: string; args: unknown[] }) => {
      calls.push({ method: event.method, args: event.args });
      exchange.emitter.emit('callMethod:done', { requestId: event.requestId, success: false,
        result: { message: 'synthetic uncertain response', outcome: 'unknown', kind: 'network' } });
    });
    await expect(exchange.placeOrder(placed)).rejects.toMatchObject({ outcome: 'unknown' });
    expect(calls).toEqual([{ method: 'placeOrder', args: [placed] }]);
    expect(fake.constructed).not.toHaveBeenCalled();
  });

  it('keeps the existing single retry for an interrupted public candle read', async () => {
    const fake = fakeModule('bitrue', false);
    setWasmEngineLoader(async () => fake.module);
    const { exchange, calls, bridge } = mirror('bitrue');
    exchange.off('callMethod', bridge);
    exchange.on('callMethod', (event: { requestId: string; method: string; args: unknown[] }) => {
      calls.push({ method: event.method, args: event.args });
      exchange.emitter.emit('callMethod:done', {
        requestId: event.requestId, success: calls.length > 1,
        result: calls.length > 1 ? [] : { message: 'synthetic retired public read', code: 'WORKER_STOPPED' },
      });
    });
    await expect(exchange.fetchOHLCV({ symbol: 'BTCUSDT', interval: '1m' })).resolves.toEqual([]);
    expect(calls.map(({ method }) => method)).toEqual(['fetchOHLCV', 'fetchOHLCV']);
  });
});
