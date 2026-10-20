import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';

import { afterEach, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

afterEach(() => vi.unstubAllEnvs());

it.each([
  ['production', false, true],
  ['production', true, true],
  ['development', true, true],
  ['development', false, false],
] as const)('BloFin broker attribution preserves TS policy in %s with proxy=%s', async (environment, tealstreetProxy, attributed) => {
  vi.stubEnv('NODE_ENV', environment);
  const noop = () => '[]';
  const engine: WasmEngineLike = {
    start: noop, command: noop, takeFrame: () => undefined,
    snapshotFrame: () => new Uint8Array(), wsOpen: noop, wsMessage: noop, wsMessages: noop,
    wsClose: noop, httpStart: noop, httpChunk: noop, httpEnd: noop,
    httpFailed: noop, timer: noop,
  };
  const construct = vi.fn(function (_venue: string, _options: string) { return engine; });
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const store = new DefaultStore();
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
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
