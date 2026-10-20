import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { RustEngineExchange } from '../../exchanges/rust/rust.exchange';

import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { DefaultStore } from '../../store/store.base';
import { directTransport } from '../../transport/direct';

const wasm = vi.hoisted(() => ({ initialize: vi.fn(), generation: 0, engines: [] as WasmEngineLike[] }));

vi.mock('@tealstreet/safe-cex-wasm/engine', () => {
  const engineModule = {
    WasmEngine: class {
      private readonly generation = wasm.generation;

      constructor() {
        wasm.engines.push(this);
      }

      start = () => '';
      command = () => '';
      takeFrame = () => undefined;
      snapshotFrame = () => {
        if (this.generation !== wasm.generation) throw new Error('WASM instance replaced under a live engine');
        return Uint8Array.of(this.generation);
      };
      wsOpen = () => '';
      wsMessage = () => '';
      wsClose = () => '';
      httpStart = () => '';
      httpChunk = () => '';
      httpEnd = () => '';
      httpFailed = () => '';
      timer = () => '';
    },
  };
  return {
    loadEngine: async () => {
      await wasm.initialize();
      return engineModule;
    },
  };
});

const accounts: RustEngineExchange[] = [];

beforeEach(() => {
  vi.resetModules();
  wasm.generation = 0;
  wasm.engines.length = 0;
});

afterEach(() => {
  for (const account of accounts.splice(0)) account.dispose();
});

const setup = async () => {
  // Import a fresh wrapper so each test exercises the actual default loader,
  // without exporting a test-only reset or replacing that loader with a mock.
  const sdk = await import('../../exchanges/rust/rust.exchange');
  sdk.setRustEngineTransport(() => ({
    now: () => 1000,
    request: async () => ({ status: 200, body: new Uint8Array() }),
    connect: () => ({ send() {}, close() {} }),
    setTimeout: () => 0,
    clearTimeout() {},
  }));
  const account = () => {
    const store = new DefaultStore();
    store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
    const exchange = new sdk.RustEngineExchange(
      'binance',
      { key: '', secret: '', publicDataOnly: true, transport: directTransport },
      store,
    );
    accounts.push(exchange);
    return exchange;
  };
  return { ...sdk, account };
};

it('shares default WASM initialization across concurrent accounts and preserves existing engines', async () => {
  let initialize!: () => void;
  const pending = new Promise<void>((resolve) => {
    initialize = resolve;
  });
  wasm.initialize.mockImplementation(async () => {
    await pending;
    wasm.generation++;
  });
  const { account } = await setup();
  const first = account();
  const second = account();
  const started = Promise.all([first.start(), second.start()]);
  await vi.waitFor(() => expect(wasm.initialize).toHaveBeenCalled());
  expect(wasm.engines).toHaveLength(0);
  initialize();
  await started;
  expect(wasm.initialize).toHaveBeenCalledTimes(1);
  expect(wasm.engines).toHaveLength(2);
  expect(new Uint8Array(first.snapshotFrame()!)).toEqual(Uint8Array.of(1));
  expect(new Uint8Array(second.snapshotFrame()!)).toEqual(Uint8Array.of(1));

  const later = account();
  await later.start();
  expect(wasm.initialize).toHaveBeenCalledTimes(1);
  expect(new Uint8Array(first.snapshotFrame()!)).toEqual(Uint8Array.of(1));
  expect(new Uint8Array(later.snapshotFrame()!)).toEqual(Uint8Array.of(1));
});

it('lets a new account retry failed default initialization while failed accounts retain their startup result', async () => {
  const failure = new Error('WASM initialization failed');
  wasm.initialize.mockRejectedValueOnce(failure).mockImplementation(async () => {
    wasm.generation++;
  });
  const { account } = await setup();
  const first = account();
  const second = account();
  const startup = first.start();
  expect(await Promise.allSettled([startup, second.start()])).toEqual([
    { status: 'rejected', reason: failure },
    { status: 'rejected', reason: failure },
  ]);
  expect(wasm.initialize).toHaveBeenCalledTimes(1);
  expect(wasm.engines).toHaveLength(0);

  const retry = account();
  await retry.start();
  expect(wasm.initialize).toHaveBeenCalledTimes(2);
  expect(first.start()).toBe(startup);
  await expect(first.start()).rejects.toBe(failure);
  await expect(second.start()).rejects.toBe(failure);
  expect(new Uint8Array(retry.snapshotFrame()!)).toEqual(Uint8Array.of(1));
  await account().start();
  expect(wasm.initialize).toHaveBeenCalledTimes(2);
  expect(new Uint8Array(retry.snapshotFrame()!)).toEqual(Uint8Array.of(1));
});

it('keeps custom loader invocation under the registered loader’s control', async () => {
  const { account, setWasmEngineLoader } = await setup();
  const { loadEngine } = await import('@tealstreet/safe-cex-wasm/engine');
  const module = await loadEngine();
  wasm.initialize.mockClear();
  const custom = vi.fn(async () => module);
  setWasmEngineLoader(custom);
  await Promise.all([account().start(), account().start()]);
  expect(custom).toHaveBeenCalledTimes(2);
  expect(wasm.initialize).not.toHaveBeenCalled();
});
