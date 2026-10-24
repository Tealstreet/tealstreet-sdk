import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import { afterEach, expect, it } from 'vitest';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const running: RustEngineExchange[] = [];
afterEach(() => {
  running.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
});

it('moves both Phemex signing slots into the private setup channel before construction', async () => {
  const calls: string[][] = [];
  class Engine {
    constructor(...args: string[]) {
      calls.push(args);
      throw new Error('construction stop');
    }
  }
  setWasmEngineLoader(async () => ({ WasmEngine: Engine }) as unknown as WasmEngineModule);
  const options = {
    key: 'fixture-primary', secret: 'audit-secret-sentinel',
    secondary_key: 'fixture-secondary', secondary_secret: 'audit-read-sentinel',
  };
  const exchange = new RustEngineExchange('phemex', options, new DefaultStore());
  running.push(exchange);
  expect(JSON.stringify(exchange.options)).not.toContain('audit-secret-sentinel');
  expect(JSON.stringify(exchange.options)).not.toContain('audit-read-sentinel');
  await expect((exchange as unknown as { ensureHost(): Promise<void> }).ensureHost()).rejects.toThrow('construction stop');
  expect(calls).toHaveLength(1);
  expect(calls[0][0]).toBe('phemex');
  const generic = JSON.parse(calls[0][1]);
  expect(generic.key).toBeUndefined();
  expect(generic.secret).toBeUndefined();
  expect(generic.readCredentials).toBeUndefined();
  expect(JSON.parse(calls[0][2])).toEqual({
    primary: { key: 'fixture-primary', secret: 'audit-secret-sentinel' },
    secondary: { key: 'fixture-secondary', secret: 'audit-read-sentinel' },
  });
  expect(options.secret).toBe('audit-secret-sentinel');
});
