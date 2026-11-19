import { afterEach, expect, it, vi } from 'vitest';

import fixtures from '../../../sdk-rs/engine/tests/fixtures/store-frames.json';

afterEach(() => vi.unstubAllGlobals());

it('imports and applies native account frames with WebAssembly unavailable', async () => {
  vi.stubGlobal('WebAssembly', undefined);
  vi.resetModules();
  const mobile = await import('./index');
  const { Frame } = await import('@tealstreet/safe-cex-wasm/wire');
  const store = new mobile.FrameStore();
  store.ingest(new Frame(Uint8Array.from(fixtures.ready).buffer), { snapshot: true, seq: 1 });
  expect(store.hasMarketIdentity).toBe(true);
  expect(store.hasAccountState).toBe(true);
  expect(store.latency).toBe(0);
  expect(store.options.isHedged).toBe(true);
  expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 15, sellLeverage: 30 });
  expect(mobile).not.toHaveProperty('setReactNativeRustEngineLoader');
});
