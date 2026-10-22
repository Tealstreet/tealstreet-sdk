import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import { afterEach, expect, it } from 'vitest';
import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const running: RustEngineExchange[] = [];
afterEach(() => {
  running.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(nativeTransport);
});

it.each(['record', 'host'] as const)('releases local provider allocation when %s construction fails', async (failure) => {
  let frees = 0;
  const calls: string[][] = [];
  class Engine {
    constructor(...args: string[]) { calls.push(args); }
    construction() { return failure === 'record' ? '{' : '{}'; }
    requiresExternalSigner() { return true; }
    free() { frees++; throw new Error('cleanup detail'); }
  }
  setWasmEngineLoader(async () => ({ WasmEngine: Engine }) as unknown as WasmEngineModule);
  setRustEngineTransport(() => ({
    now: () => 1000,
    request: async () => { throw new Error('unexpected request'); },
    connect: () => { throw new Error('unexpected socket'); },
    setTimeout: () => 0,
    clearTimeout() {},
  }));
  const exchange = new RustEngineExchange('binance', { key: 'public-key-only', secret: 'audit-secret-sentinel' }, new DefaultStore());
  running.push(exchange);
  const error = await (exchange as unknown as { ensureHost(): Promise<void> }).ensureHost().then(() => undefined, (error: Error) => error);
  expect(error).toBeInstanceOf(Error);
  expect(error!.message).not.toContain('cleanup detail');
  expect(frees).toBe(1);
  expect(calls).toHaveLength(1);
  expect(calls[0][1]).not.toContain('audit-secret-sentinel');
  expect(JSON.parse(calls[0][2])).toEqual({ primary: { key: 'public-key-only', secret: 'audit-secret-sentinel' } });
  exchange.dispose();
  expect(frees).toBe(1);
});
