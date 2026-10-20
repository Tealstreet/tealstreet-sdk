import type { RustEngineExchange } from '../../exchanges/rust/rust.exchange';

import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { DefaultStore } from '../../store/store.base';
import { directTransport } from '../../transport/direct';

const wasm = vi.hoisted(() => ({ loads: [] as string[], engines: [] as string[] }));

vi.mock('@tealstreet/safe-cex-wasm/engine', () => {
  const engineModule = {
    WasmEngine: class {
      constructor(venue: string) {
        wasm.engines.push(venue);
      }

      start = () => '';
      command = () => '';
      takeFrame = () => undefined;
      snapshotFrame = () => Uint8Array.of(1);
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
    loadEngine: async (venue: string) => {
      wasm.loads.push(venue);
      return engineModule;
    },
  };
});

const accounts: RustEngineExchange[] = [];

beforeEach(() => {
  vi.resetModules();
  wasm.loads.length = 0;
  wasm.engines.length = 0;
});

afterEach(() => {
  for (const account of accounts.splice(0)) account.dispose();
});

const setup = async () => {
  // Import a fresh wrapper so each test exercises the actual default loader.
  const sdk = await import('../../exchanges/rust/rust.exchange');
  sdk.setRustEngineTransport(() => ({
    now: () => 1000,
    request: async () => ({ status: 200, body: new Uint8Array() }),
    connect: () => ({ send() {}, close() {} }),
    setTimeout: () => 0,
    clearTimeout() {},
  }));
  const account = (name: 'binance' | 'bybitspot' | 'tealstreet-dex' = 'binance') => {
    const store = new DefaultStore();
    store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
    const exchange = new sdk.RustEngineExchange(
      name,
      { key: '', secret: '', publicDataOnly: true, transport: directTransport },
      store,
    );
    accounts.push(exchange);
    return exchange;
  };
  return { ...sdk, account };
};

// Per-family sharing and retry of initialization: hosts/wasm/js/family-loader.test.mjs.
it('asks the default loader for each account’s Rust venue', async () => {
  const { account } = await setup();
  // Sequential: concurrent first imports of a mocked module can bypass the mock.
  await account('binance').start();
  await account('bybitspot').start();
  expect(wasm.loads.sort()).toEqual(['binance', 'bybitspot']);
  expect(wasm.engines.sort()).toEqual(['binance', 'bybitspot']);
});

it('keeps custom loader invocation under the registered loader’s control', async () => {
  const { account, setWasmEngineLoader } = await setup();
  const { loadEngine } = await import('@tealstreet/safe-cex-wasm/engine');
  const module = await loadEngine('binance');
  wasm.loads.length = 0;
  const custom = vi.fn(async (_venue: string) => module);
  setWasmEngineLoader(custom);
  await Promise.all([account().start(), account('bybitspot').start()]);
  expect(custom.mock.calls.map(([venue]) => venue).sort()).toEqual(['binance', 'bybitspot']);
  expect(wasm.loads).toEqual([]);
});
