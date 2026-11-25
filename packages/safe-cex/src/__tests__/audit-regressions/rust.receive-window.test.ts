import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';

import { afterEach, expect, it, vi } from 'vitest';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { FrameStore } from '../../store/frameStore';
import { engineCapabilities } from '../fixtures/engineCapabilities';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

const running: RustEngineExchange[] = [];
afterEach(() => {
  running.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(nativeTransport);
});

it.each([
  'binance',
  'binancepm',
  'binancespot',
  'binanceinverse',
  'bybit',
  'bybitinverse',
  'bybitspot',
  'bybitdemo',
  'mexc',
] as const)('preserves a caller-selected short receive window for %s', async (name) => {
  const construct = vi.fn();
  class Engine {
    constructor(venue: string, options: string) {
      construct(venue, JSON.parse(options));
    }
    syncHostTime() {
      return true;
    }
    capabilities() {
      return engineCapabilities(name);
    }
    free() {}
  }
  setWasmEngineLoader(async () => ({ WasmEngine: Engine }) as unknown as WasmEngineModule);
  const exchange = new RustEngineExchange(
    name,
    { key: '', secret: '', publicDataOnly: true, extra: { recvWindow: 5000 } },
    new FrameStore(),
  );
  running.push(exchange);
  await (exchange as unknown as { ensureHost(): Promise<void> }).ensureHost();
  expect(construct).toHaveBeenCalledExactlyOnceWith(name, expect.objectContaining({ recvWindow: 5000 }));
});
