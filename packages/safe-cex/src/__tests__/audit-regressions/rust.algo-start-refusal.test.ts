import { afterEach, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

afterEach(() => vi.restoreAllMocks());

it.each(['twap:start', 'chaser:start', 'exec'])('refuses %s when owner-bound startup throws', (op) => {
  const exchange = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
  const run = vi.fn();
  const refuse = vi.fn();
  const host = {
    algoStart: vi.fn<() => void>(() => {
      throw new Error('authenticated owner changed during startup');
    }),
    algosRunning: true,
    algoDispose: vi.fn(async () => {}),
    dispose: vi.fn(),
  };
  Reflect.set(exchange, 'host', host);
  try {
    expect(() => exchange.routeAlgoOp('acct', op, run, refuse)).not.toThrow();
    expect(run).not.toHaveBeenCalled();
    expect(refuse).toHaveBeenCalledExactlyOnceWith(`${op} was not run: authenticated owner changed during startup`);
    host.algoStart.mockImplementation(() => {});
    exchange.routeAlgoOp('acct', op, run, refuse);
    expect(run).toHaveBeenCalledTimes(1);
    expect(refuse).toHaveBeenCalledTimes(1);
  } finally {
    exchange.dispose();
  }
});
