// @vitest-environment node
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { Market } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { BaseExchange } from '../../exchanges/base';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { setTickIntervalMultipliers } from '../../utils/tickIntervalOverride';
import { virtualClock } from '../../utils/virtualClock';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from './consumerFrame';
import { realVenue } from './realVenue';

const accounts: BaseExchange[] = [];
const inputs: ReturnType<typeof realVenue>[] = [];
afterEach(() => {
  accounts.splice(0).forEach((exchange) => exchange.dispose());
  inputs.splice(0).forEach((input) => input.dispose());
  virtualClock.stop();
  setWasmEngineLoader(loadNodeEngine);
  setTickIntervalMultipliers(undefined);
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it.each(['setTickerSymbols', 'setPollScale'] as const)(
  'contains a replyless %s trap without an unhandled rejection or duplicate error',
  async (operation) => {
    vi.useFakeTimers();
    vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.spyOn(console, 'trace').mockImplementation(() => {});
    let shouldTrap = false;
    const idle = () => '[]';
    const venue = operation === 'setPollScale' ? 'breakoutprop' : 'binance';
    const source: WasmEngineLike = {
      capabilities: () => engineCapabilities(venue),
      start: idle,
      wsOpen: idle,
      wsMessage: idle,
      wsClose: idle, wsRouteExhausted: idle,
      httpStart: idle,
      httpChunk: idle,
      httpEnd: idle,
      httpFailed: idle,
      timer: idle,
      command: (json) => {
        if (shouldTrap && JSON.parse(json).op === operation) panic();
        return '[]';
      },
      takeFrame: () =>
        new Uint8Array(
          consumerFrame({
            markets: [],
            tickers: [],
            loaded: { markets: true, tickers: true, balance: false, orders: false, positions: false },
          }),
        ),
      snapshotFrame: () => new Uint8Array(consumerFrame({ markets: [], tickers: [] })),
    };
    setWasmEngineLoader(async () => ({
      WasmEngine: function () {
        return source;
      } as unknown as WasmEngineModule['WasmEngine'],
    }));
    setRustEngineTransport(() => ({
      now: Date.now,
      connect: () => {
        throw new Error('Control fixture must not connect');
      },
      request: async () => {
        throw new Error('Control fixture must not request');
      },
      setTimeout,
      clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
    }));
    const exchange = new RustEngineExchange(
      venue,
      { key: 'fixture-user', secret: 'fixture-password', publicDataOnly: true },
      new DefaultStore(),
    );
    accounts.push(exchange);
    const error = vi.fn();
    const dead = vi.fn();
    exchange.on('error', error);
    exchange.on('engine-dead', dead);
    const startup = expect(exchange.start()).resolves.toBeUndefined();
    await vi.advanceTimersByTimeAsync(250);
    await startup;
    shouldTrap = true;
    if (operation === 'setTickerSymbols') exchange.setActiveTickerSymbols(['BTCUSDT']);
    else setTickIntervalMultipliers({ breakoutprop: 2 });
    await vi.advanceTimersByTimeAsync(1000);
    expect(exchange.isEngineDead).toBe(true);
    expect(exchange.isDisposed).toBe(true);
    expect(error).toHaveBeenCalledTimes(1);
    expect(dead).toHaveBeenCalledTimes(1);
  },
);

// A real unreachable instruction, without a production engine panic endpoint.
const panic = new WebAssembly.Instance(
  new WebAssembly.Module(
    Uint8Array.from([
      0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0, 7, 8, 1, 4, 116, 114, 97, 112, 0, 0, 10, 5, 1, 3, 0,
      0, 11,
    ]),
  ),
).exports.trap as () => never;

it('contains a trap, wakes failed startup, and keeps a sibling native account usable', async () => {
  const module = await loadNodeEngine();
  const engines: WasmEngineLike[] = [];
  const fail = new Set<WasmEngineLike>();
  // Keep real native objects and module initialization; intercept just the
  // entry boundary to reproduce a runtime trap deterministically.
  const Wrapped = function (venue: string, options: string, signerSetupJson?: string) {
    const native = new module.WasmEngine(venue, options, signerSetupJson);
    engines.push(native);
    return new Proxy(native, {
      get(target, key) {
        const value = Reflect.get(target, key);
        if (typeof value !== 'function') return value;
        if (key === 'takeFrame' || key === 'start')
          return (...args: unknown[]) => {
            if (fail.has(native)) panic();
            return Reflect.apply(value, target, args);
          };
        return value.bind(target);
      },
    });
  } as unknown as WasmEngineModule['WasmEngine'];
  setWasmEngineLoader(async () => ({ WasmEngine: Wrapped }));
  vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.spyOn(console, 'trace').mockImplementation(() => {});
  const firstInput = realVenue('binance');
  const secondInput = realVenue('binance');
  inputs.push(firstInput, secondInput);
  const transports = [firstInput.transport, secondInput.transport];
  setRustEngineTransport(() => transports.shift()!);
  const first = new RustEngineExchange('binance', firstInput.options, new DefaultStore());
  const sibling = new RustEngineExchange('binance', secondInput.options, new DefaultStore());
  accounts.push(first, sibling);
  const errors: unknown[] = [];
  const dead: Error[] = [];
  first.on('error', (error: unknown) => errors.push(error));
  first.on('engine-dead', (error: Error) => dead.push(error));
  await Promise.all([first.start(), sibling.start()]);
  const count = sibling.store.markets.length;
  expect(count).toBeGreaterThan(0);
  fail.add(engines[0]);
  await vi.waitFor(() => expect(first.isEngineDead).toBe(true));
  expect(first.isDisposed).toBe(true);
  expect(errors).toHaveLength(1);
  expect(dead).toHaveLength(1);
  expect(dead[0]).toMatchObject({ code: 'ENGINE_DEAD' });
  expect(sibling.isEngineDead).toBe(false);
  expect(new Frame(sibling.snapshotFrame()!).markets()!.count).toBe(count);
  await expect(first.fetchOHLCV({ symbol: 'BTCUSDT', interval: '1m' })).rejects.toMatchObject({ code: 'ENGINE_DEAD' });

  const startupInput = realVenue('binance');
  inputs.push(startupInput);
  setRustEngineTransport(() => startupInput.transport);
  setWasmEngineLoader(async () => ({
    WasmEngine: class extends Wrapped {
      constructor(venue: string, options: string, signerSetupJson?: string) {
        super(venue, options, signerSetupJson);
        fail.add(engines.at(-1)!);
      }
    },
  }));
  const startup = new RustEngineExchange('binance', startupInput.options, new DefaultStore());
  accounts.push(startup);
  await expect(startup.start()).rejects.toMatchObject({ code: 'ENGINE_DEAD' });
  expect(startup.isDisposed).toBe(true);
  expect(startup.isEngineDead).toBe(true);
  expect(sibling.store.loaded.tickers).toBe(true);
});

it('starts a new forwarded stream with a native empty snapshot, clearing old book baselines', async () => {
  const input = realVenue('binance');
  inputs.push(input);
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => input.transport);
  const market: Market = {
    id: 'BTCUSDT',
    symbol: 'BTCUSDT',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    precision: { amount: 0.001, price: 0.1 },
    limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 125 } },
  };
  const main = new BaseExchange('binance', { key: '', secret: '' }, new DefaultStore());
  const exchange = new RustEngineExchange('binance', input.options, new DefaultStore());
  accounts.push(main, exchange);
  main.applyWorkerFrame(consumerFrame({ markets: [market] }, undefined, true));
  const received: Frame[] = [];
  exchange.onFrame = (buffer) => {
    const copy = structuredClone(buffer, { transfer: [buffer] });
    received.push(new Frame(copy));
    main.applyWorkerFrame(copy);
  };
  await exchange.start();
  expect(received[0].markets()!.count).toBe(0);
  expect(received[0].status()!.loaded).toBe(0);
  expect(main.store.loaded.markets).toBe(true);

  // Identical instrument identity must still require a new book snapshot.
  main.applyWorkerFrame(consumerFrame({ markets: [market] }));
  const delta = new ArrayBuffer(56);
  const header = new DataView(delta);
  header.setUint32(0, 0x46584353, true);
  header.setUint16(4, 1, true);
  header.setUint16(6, 1, true);
  header.setUint16(16, 15, true);
  header.setUint32(20, 32, true);
  header.setUint32(28, 1, true);
  header.setFloat64(40, 99, true);
  header.setFloat64(48, 9, true);
  // The main thread rejects the stale-baseline delta and requests a fresh snapshot.
  const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
  expect(() => main.applyWorkerFrame(delta)).not.toThrow();
  expect(errors).toHaveBeenCalledWith(
    'Engine frame apply failed',
    expect.objectContaining({ message: expect.stringMatching(/snapshot required/) }),
  );
});
