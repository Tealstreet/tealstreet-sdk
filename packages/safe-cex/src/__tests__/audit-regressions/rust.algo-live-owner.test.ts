import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { TwapOwner } from '../../webworker/twap/TwapOwner';

import { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import { afterEach, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

afterEach(() => vi.useRealTimers());

it.each(['unchanged', 'generation', 'returning'] as const)('carries the initiating owner through engine readiness: %s', (transition) => {
  let owner: TwapOwner = { ownerId: 'A', generation: 7 };
  const exchange = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
  exchange.setTwapOwnerProvider(() => owner);
  const run = vi.fn();
  const refused = vi.fn();
  exchange.routeAlgoOp('acct', 'exec', run, refused);
  if (transition === 'generation') owner = { ownerId: 'A', generation: 8 };
  if (transition === 'returning') {
    owner = { ownerId: 'B', generation: 8 };
    owner = { ownerId: 'A', generation: 9 };
  }
  Reflect.set(exchange, 'host', {
    algoStart: vi.fn(), algosRunning: true, algoDispose: vi.fn(async () => {}), dispose: vi.fn(),
  });
  try {
    exchange.startAlgos('acct');
    expect(run).toHaveBeenCalledTimes(transition === 'unchanged' ? 1 : 0);
    expect(refused).toHaveBeenCalledTimes(transition === 'unchanged' ? 0 : 1);
  } finally {
    exchange.dispose();
  }
});

it.each(['unchanged', 'generation', 'returning', 'logout'] as const)(
  'checks the creation binding on the cached algo route: %s',
  (transition) => {
    let owner: TwapOwner | null = { ownerId: 'A', generation: 7 };
    const exchange = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
    const nativeOp = vi.fn(() => '[]');
    const free = vi.fn();
    const host = new EngineHost({
      algoStart: vi.fn(),
      algoStartOwned: vi.fn(),
      algoRunning: () => true,
      algoDeadline: () => undefined,
      algoOp: nativeOp,
      algoEmits: () => '[]',
      free,
    } as unknown as WasmEngineLike, {
      now: () => 0, setTimeout, clearTimeout,
      connect: () => { throw new Error('unexpected socket'); },
      request: async () => { throw new Error('unexpected request'); },
    }, {}, { ownsEngine: true });
    Reflect.set(exchange, 'host', host);
    exchange.setTwapOwnerProvider(() => owner);
    expect(exchange.startAlgos('acct')).toBe(true);
    if (transition === 'generation') owner = { ownerId: 'A', generation: 8 };
    if (transition === 'returning') {
      owner = { ownerId: 'B', generation: 8 };
      owner = { ownerId: 'A', generation: 9 };
    }
    if (transition === 'logout') owner = null;
    const refused = vi.fn();
    const run = vi.fn(() => exchange.algoOp('twap:state', [{ ownerId: 'A', ownerGeneration: 7 }]));
    try {
      exchange.routeAlgoOp('acct', 'twap:state', run, refused);
      if (transition === 'unchanged') {
        expect(run).toHaveBeenCalledOnce();
        expect(nativeOp).toHaveBeenCalledOnce();
        expect(refused).not.toHaveBeenCalled();
      } else {
        expect(run).not.toHaveBeenCalled();
        expect(nativeOp).not.toHaveBeenCalled();
        expect(refused).toHaveBeenCalledOnce();
        expect(host.dead).toBe(true);
        expect(free).toHaveBeenCalledOnce();
      }
    } finally {
      host.dispose();
      exchange.dispose();
    }
  },
);
