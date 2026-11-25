import { afterEach, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { FrameStore } from '../../store/frameStore';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

afterEach(() => vi.restoreAllMocks());

// Rule 5: display labels change while the original operations and refusal controls stay pinned.
it.each([
  ['twap:start', 'Starting the TWAP'],
  ['chaser:start', 'Starting the chase'],
  ['exec', 'Running this trading request'],
])('refuses %s when owner-bound startup throws', (op, label) => {
  const exchange = new RustEngineExchange('bybit', { key: '', secret: '' }, new FrameStore());
  const run = vi.fn();
  const refuse = vi.fn();
  const host = {
    algoStart: vi.fn<() => void>(() => {
      throw new Error('authenticated owner changed during startup');
    }),
    algosRunning: true,
    // Status is observational; a successful retry still needs execution admission.
    checkAlgosForExecution: vi.fn(() => true),
    algoDispose: vi.fn(async () => {}),
    dispose: vi.fn(),
  };
  Reflect.set(exchange, 'host', host);
  try {
    expect(() => exchange.routeAlgoOp('acct', op, run, refuse)).not.toThrow();
    expect(run).not.toHaveBeenCalled();
    expect(refuse).toHaveBeenCalledExactlyOnceWith(`${label} wasn’t possible: authenticated owner changed during startup`);
    expect(host.checkAlgosForExecution).not.toHaveBeenCalled();
    host.algoStart.mockImplementation(() => {});
    exchange.routeAlgoOp('acct', op, run, refuse);
    expect(run).toHaveBeenCalledTimes(1);
    expect(refuse).toHaveBeenCalledTimes(1);
    expect(host.checkAlgosForExecution).toHaveBeenCalledExactlyOnceWith();
    host.checkAlgosForExecution.mockReturnValue(false);
    exchange.routeAlgoOp('acct', op, run, refuse);
    expect(run).toHaveBeenCalledTimes(1);
    expect(refuse).toHaveBeenCalledTimes(2);
    expect(refuse).toHaveBeenLastCalledWith(`${label} wasn’t possible because bybit was interrupted`);
  } finally {
    exchange.dispose();
  }
});
