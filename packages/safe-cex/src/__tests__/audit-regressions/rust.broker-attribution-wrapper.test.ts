import { installNativeStartupFrames } from './rust.native-startup.fixtures';
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';

import { afterEach, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from '../cutover/consumerFrame';

afterEach(() => vi.unstubAllEnvs());

it.each([
  ['production', false, true],
  ['production', true, true],
  ['development', true, true],
  ['development', false, false],
] as const)('BloFin broker attribution preserves TS policy in %s with proxy=%s', async (environment, tealstreetProxy, attributed) => {
  vi.stubEnv('NODE_ENV', environment);
  const noop = () => '[]';
  const nativeLoaded = { markets: true, tickers: true, balance: true, orders: true, positions: true };
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities('blofin'),
    start: noop, command: noop, takeFrame: () => new Uint8Array(consumerFrame({ loaded: nativeLoaded })),
    snapshotFrame: () => new Uint8Array(), wsOpen: noop, wsMessage: noop,
    wsClose: noop, wsRouteExhausted: noop, httpStart: noop, httpChunk: noop, httpEnd: noop,
    httpFailed: noop, timer: noop,
  };
  const construct = vi.fn(function (_venue: string, _options: string) { return engine; });
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const store = new DefaultStore();
  installNativeStartupFrames(engine, store);
  store.update({ loaded: nativeLoaded });
  const exchange = new RustEngineExchange('blofin', {
    key: 'fixture-key', secret: 'fixture-secret', passphrase: 'fixture-pass', tealstreetProxy,
  }, store);
  try {
    await exchange.start();
    expect(JSON.parse(construct.mock.calls[0][1]).brokerAttribution).toBe(attributed);
  } finally {
    exchange.dispose();
  }
});
