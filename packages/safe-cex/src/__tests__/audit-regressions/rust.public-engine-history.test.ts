vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import { describe, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

/**
 * A public-only account's history was NOT read, which is not the same as empty: ingestion
 * would close the window and lose the fills. Asking the engine instead raised an error
 * toast and a Sentry event for a user after a key takeover (2026-11-02).
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
  it.each(READS)('%s rejects as not read, reaches nothing and reports nothing', async (...read) => {
    const exchange = new RustEngineExchange(
      'bitget',
      { key: '', secret: '', publicDataOnly: true, ...shape },
      new DefaultStore(),
    );
    const forwarded = vi.fn();
    const errors = vi.fn();
    exchange.on('callMethod', forwarded);
    exchange.on('error', errors);

    await expect(call(exchange, read)).rejects.toMatchObject({ outcome: 'notSent' });

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
