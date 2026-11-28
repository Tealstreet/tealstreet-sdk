import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeOptions } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { FrameStore } from '../../store/frameStore';

// Aster serves no V1 order routes since its 2026-11 V1 removal, so a V1 key is refused when added.
const running: RustEngineExchange[] = [];
afterEach(() => {
  running.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(nativeTransport);
});

const engineLoads = () => {
  const load = vi.fn(async (): Promise<WasmEngineModule> => {
    throw new Error('engine not needed');
  });
  setWasmEngineLoader(load);
  setRustEngineTransport((() => {
    throw new Error('I/O must not start');
  }) as never);
  return load;
};

const exchange = (options: Partial<ExchangeOptions>) => {
  const created = new RustEngineExchange(
    'asterdex',
    { key: 'synthetic-key', secret: 'synthetic-material', ...options } as ExchangeOptions,
    new FrameStore(),
  );
  running.push(created);
  return created;
};

it('refuses a V1 key at validation with the V3 replacement, before any engine or I/O', async () => {
  const load = engineLoads();
  const result = await exchange({}).validateAccount();
  expect(result).toEqual({ error: expect.stringContaining('V3 API wallet'), kind: 'authInvalid' });
  expect(load).not.toHaveBeenCalled();
});

it('still validates a V3 key through its engine', async () => {
  const load = engineLoads();
  const result = await exchange({ extra: { asterdexApiVersion: 'v3' } }).validateAccount();
  expect(load).toHaveBeenCalled();
  expect(result.error).not.toContain('V3 API wallet');
});
