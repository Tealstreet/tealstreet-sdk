import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';

import { afterEach, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { installNativeStartupFrames } from './rust.native-startup.fixtures';

const running: RustEngineExchange[] = [];
afterEach(() => {
  for (const exchange of running.splice(0)) exchange.dispose();
  vi.restoreAllMocks();
});

it.each([undefined, 'BRA_BTL'] as const)('forwards Bybit site %s to native auth-diagnostic eligibility', async (bybitSiteId) => {
  const noop = () => '[]';
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities('bybit'),
    start: noop,
    command: noop,
    takeFrame: () => undefined,
    snapshotFrame: () => new Uint8Array(),
    wsOpen: noop,
    wsMessage: noop,
    wsClose: noop, wsRouteExhausted: noop,
    httpStart: noop,
    httpChunk: noop,
    httpEnd: noop,
    httpFailed: noop,
    timer: noop,
  };
  let captured: Record<string, unknown> | undefined;
  setWasmEngineLoader(async () => ({
    WasmEngine: vi.fn(function (_name: string, options: string) {
      captured = JSON.parse(options);
      return engine;
    }) as unknown as WasmEngineModule['WasmEngine'],
  }));
  const store = new DefaultStore();
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  installNativeStartupFrames(engine, store);
  const exchange = new RustEngineExchange('bybit', { key: 'key', secret: 'secret', bybitSiteId }, store);
  running.push(exchange);
  await exchange.start();
  expect(captured).toBeDefined();
  expect(captured?.bybitSiteId).toBe(bybitSiteId);
});
