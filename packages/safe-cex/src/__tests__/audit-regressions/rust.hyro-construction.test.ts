vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { WasmEngineLike, Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';

import { afterEach, expect, it, vi } from 'vitest';
import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import {
  HYROTRADER_PROD_URL, HYROTRADER_PROD_WS_URL,
  HYROTRADER_SIMULATOR_URL, HYROTRADER_SIMULATOR_WS_URL,
} from '../../exchanges/bybit/bybit.types';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { engineCapabilities } from '../fixtures/engineCapabilities';

const running: RustEngineExchange[] = [];
afterEach(() => {
  running.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(nativeTransport);
});

it.each([undefined, 'demo', 'production'] as const)('forwards raw Hyro mode %s and both network configs uniformly', async (mode) => {
  const construct = vi.fn();
  const io = vi.fn(() => { throw new Error('No I/O expected during host construction'); });
  const transport: Transport = {
    now: () => Date.now(), connect: io, request: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  setRustEngineTransport(() => transport);
  for (const name of ['hyrotrader', 'binance'] as const) {
    const engine = { capabilities: () => engineCapabilities(name) } as WasmEngineLike;
    setWasmEngineLoader(async () => ({
      WasmEngine: function (venue: string, options: string) { construct(venue, JSON.parse(options)); return engine; },
    }) as unknown as WasmEngineModule);
    for (const testnet of [false, true]) {
      const exchange = new RustEngineExchange(name, {
        key: '', secret: '', publicDataOnly: true, testnet,
        extra: { hyrotraderMode: mode } as ExchangeOptions['extra'],
      }, new DefaultStore());
      running.push(exchange);
      await (exchange as unknown as { ensureHost(): Promise<void> }).ensureHost();
      const options = construct.mock.lastCall![1];
      expect(options).not.toHaveProperty('endpoints');
      expect(options.hyrotrader).toEqual({
        ...(mode !== undefined && { mode }),
        production: { rest: HYROTRADER_PROD_URL, publicWs: HYROTRADER_PROD_WS_URL.public.linear, privateWs: HYROTRADER_PROD_WS_URL.private },
        simulator: { rest: HYROTRADER_SIMULATOR_URL, publicWs: HYROTRADER_SIMULATOR_WS_URL.public.linear, privateWs: HYROTRADER_SIMULATOR_WS_URL.private },
      });
      expect(exchange.isDemo).toBe(name === 'hyrotrader' && mode === 'demo');
    }
  }
  expect(io).not.toHaveBeenCalled();
});
