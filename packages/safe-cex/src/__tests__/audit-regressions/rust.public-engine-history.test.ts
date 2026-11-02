vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import { describe, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

/**
 * A public-only account's history has nothing to read, as the TS venues answered.
 * Asking the engine raised "Bitget credentials are required" as an error toast and
 * a Sentry event for a user on the account after a key takeover (2026-11-02).
 */
const READS = [
  ['fetchHistoricalFills', { from: 0, to: 1 }],
  ['fetchHistoricalOrders', { from: 0, to: 1 }],
  ['fetchIncomeHistory', { startTime: 0, endTime: 1 }],
  ['fetchIncomeHistoryRange', 0, 1],
] as const;

const call = (exchange: RustEngineExchange, [method, ...args]: (typeof READS)[number]) =>
  (exchange[method] as (...input: unknown[]) => Promise<unknown>)(...args);

describe.each([
  ['the page mirror', { workerMirror: true }],
  ['the instance that owns the engine', {}],
])('history reads on a public-only account, %s', (_label, shape) => {
  it.each(READS)('%s is empty, reaches nothing and reports nothing', async (...read) => {
    const exchange = new RustEngineExchange(
      'bitget',
      { key: '', secret: '', publicDataOnly: true, ...shape },
      new DefaultStore(),
    );
    const forwarded = vi.fn();
    const errors = vi.fn();
    exchange.on('callMethod', forwarded);
    exchange.on('error', errors);

    await expect(call(exchange, read)).resolves.toEqual([]);

    expect(forwarded).not.toHaveBeenCalled();
    expect(errors).not.toHaveBeenCalled();
  });
});

it('a keyless mirror of a keyed account still asks its worker', async () => {
  const exchange = new RustEngineExchange('bitget', { key: '', secret: '', workerMirror: true }, new DefaultStore());
  const forwarded = vi.fn();
  exchange.on('callMethod', forwarded);

  void exchange.fetchHistoricalFills({ from: 0, to: 1 } as never).catch(() => undefined);

  await vi.waitFor(() => expect(forwarded.mock.calls.map(([event]) => event.method)).toEqual(['fetchHistoricalFills']));
});
