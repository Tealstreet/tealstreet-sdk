import type { RustEngineExchange } from '../../exchanges/rust/rust.exchange';

import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { DefaultStore } from '../../store/store.base';
import { directTransport } from '../../transport/direct';
import { engineCapabilities } from '../fixtures/engineCapabilities';

const wasm = vi.hoisted(() => ({ loads: [] as string[], engines: [] as string[] }));

vi.mock('@tealstreet/safe-cex-wasm/engine', () => {
  // Prove native startup from decoded tables, rather than caller-preloaded JS
  // flags. Empty validated tables keep this loader-only fixture independent of
  // venue metadata.
  const startupFrame = () => {
    const bytes = new Uint8Array(72);
    const view = new DataView(bytes.buffer);
    view.setUint32(0, 0x46584353, true);
    view.setUint16(4, 1, true);
    view.setUint16(6, 3, true);
    let at = 16;
    for (const [kind, length] of [[2, 8], [3, 8], [10, 16]]) {
      view.setUint16(at, kind, true);
      view.setUint32(at + 4, length, true);
      if (kind === 10) view.setUint32(at + 8, 31, true);
      at += 8 + length;
    }
    return bytes;
  };
  const engineModule = {
    WasmEngine: class {
      constructor(venue: string) {
        wasm.engines.push(venue);
      }

      capabilities = () => engineCapabilities('binance');
      private pending = false;
      start = () => { this.pending = true; return ''; };
      command = () => '';
      takeFrame = () => {
        if (!this.pending) return undefined;
        this.pending = false;
        return startupFrame();
      };
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
