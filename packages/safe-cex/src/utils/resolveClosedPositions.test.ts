import type { ClosedPnlEntry } from '../types';

import { describe, expect, it } from 'vitest';

import { resolveClosedPositions } from './resolveClosedPositions';

const entry = (over: Partial<ClosedPnlEntry> = {}): ClosedPnlEntry =>
  ({
    // Required on the type: a producer must declare where the number came from, so a new
    // one that forgets does not get the benefit of the doubt.
    provenance: 'venue',
    symbol: 'BTCUSDT',
    notional: 100,
    entryPrice: 10,
    exitPrice: 11,
    unrealizedPnl: 1,
    side: 'long',
    fees: 0.1,
    time: 1,
    closedTime: 2,
    account: 'acct',
    contracts: 10,
    leverage: 1,
    ...over,
  }) as ClosedPnlEntry;

const opts = { symbol: 'BTCUSDT', accountName: 'acct' } as any;

describe('resolveClosedPositions', () => {
  it('marks a venue-reported axis independently verifiable', async () => {
    const exchange = { fetchClosedPnl: async () => [entry()] };

    const result = await resolveClosedPositions(exchange, opts, false);

    expect(result.provenance).toBe('venue');
    expect(result.independentlyVerifiable).toBe(true);
    expect(result.entries[0].provenance).toBe('venue');
  });

  it('marks a folded axis derived and NOT independently verifiable', async () => {
    // This is the whole point: positions folded from fills cannot check the ledger built
    // from those same fills — they agree by construction.
    const exchange = {
      fetchClosedPnl: async () => [],
      deriveClosedPositions: async () => [entry()],
    };

    const result = await resolveClosedPositions(exchange, opts, true);

    expect(result.provenance).toBe('derived');
    expect(result.independentlyVerifiable).toBe(false);
    expect(result.entries[0].provenance).toBe('derived');
  });

  it('does not call the venue method when it is the inert base stub', async () => {
    let called = false;
    const exchange = {
      fetchClosedPnl: async () => {
        called = true;
        return [];
      },
      deriveClosedPositions: async () => [entry()],
    };

    await resolveClosedPositions(exchange, opts, true);

    expect(called).toBe(false);
  });

  it('does not overwrite what the producer declared', async () => {
    // The resolver reports the PATH it took; the row reports where its number came from.
    // A venue method handing back rows it knows are derived must not be relabelled.
    const exchange = { fetchClosedPnl: async () => [entry({ provenance: 'derived' })] };

    const result = await resolveClosedPositions(exchange, opts, false);

    expect(result.entries[0].provenance).toBe('derived');
  });

  it('a fold of a DIFFERENT source is still independently verifiable', async () => {
    // Provenance and independence are different questions. Binance folds
    // /fapi/v1/userTrades while its ledger reads /fapi/v1/income — a real cross-check,
    // even though it is derived. Conflating the two would have refused it.
    const exchange = {
      fetchClosedPnl: async () => [],
      deriveClosedPositions: async () => [entry()],
      closedPositionsShareLedgerSource: false,
    };

    const result = await resolveClosedPositions(exchange, opts, true);

    expect(result.provenance).toBe('derived');
    expect(result.independentlyVerifiable).toBe(true);
  });

  it('defaults to NOT independent when a venue forgets to declare', async () => {
    // Safe direction: an undeclared venue gets a SKIP, never a meaningless PASS.
    const exchange = { fetchClosedPnl: async () => [], deriveClosedPositions: async () => [entry()] };

    expect((await resolveClosedPositions(exchange, opts, true)).independentlyVerifiable).toBe(false);
  });

  it('reports WHY it is empty rather than returning a bare empty list', async () => {
    // An empty list with no reason is indistinguishable from "this account never traded".
    const exchange = { fetchClosedPnl: async () => [] };

    const result = await resolveClosedPositions(exchange, opts, true);

    expect(result.entries).toEqual([]);
    expect(result.unavailableReason).toBeTruthy();
  });
});
