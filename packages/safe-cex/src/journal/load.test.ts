import { describe, expect, it } from 'vitest';

import type { ExchangeName } from '../types';
import type { JournalRecord } from './select';

import { PositionSide } from '../types';

import { classifyJournalFailure, loadJournalRecords } from './load';

/**
 * A FAILED FETCH IS NOT AN ABSENCE AND NOT AN EXCLUSION.
 *
 * Same failure as `rows: 0` on an unparsed envelope, `exitPrice: 0` on an unobserved close,
 * and an excluded venue rendering as an empty tab. This is the fourth place it could appear,
 * so it gets a state of its own rather than collapsing into either neighbour.
 */

const record = (over: Partial<JournalRecord> & { exchange: ExchangeName }): JournalRecord =>
  ({
    key: 'k',
    account: 'main',
    symbol: 'BTCUSDT',
    settleCurrency: 'USDT',
    side: PositionSide.Long,
    positionSide: 'both',
    state: 'closed',
    closedSize: 1,
    avgEntryPrice: 100,
    avgExitPrice: 110,
    realizedPnl: 10,
    fees: 1,
    funding: 0,
    fundingAttributed: true,
    provenance: 'reported',
    sourceAxis: 'fills',
    closeCount: 1,
    openedAt: 1_000,
    closedAt: 2_000,
    ...over,
  }) as JournalRecord;

const source = (exchange: string, account: string, fetch: () => Promise<readonly JournalRecord[]>) => ({
  exchange: exchange as ExchangeName,
  account,
  fetch,
});

describe('loadJournalRecords', () => {
  it('reports a failed account instead of showing it as empty', async () => {
    const result = await loadJournalRecords([
      source('okx', 'ok', async () => [record({ exchange: 'okx' as ExchangeName })]),
      source('binance', 'broken', async () => {
        throw new Error('401 unauthorized');
      }),
    ]);

    expect(result.records).toHaveLength(1);
    expect(result.failed).toHaveLength(1);
    expect(result.failed[0]).toMatchObject({ exchange: 'binance', account: 'broken', kind: 'auth' });
    // The flag exists so a caller cannot forget to check `failed.length` before presenting
    // a sum as a total.
    expect(result.complete).toBe(false);
  });

  it('marks the result complete only when every account succeeded', async () => {
    const result = await loadJournalRecords([source('okx', 'ok', async () => [])]);
    expect(result.complete).toBe(true);
    expect(result.failed).toHaveLength(0);
  });

  it('does not let one failure discard the accounts that succeeded', async () => {
    // `Promise.all` would reject the whole load and show nothing, which is strictly worse
    // than a partial answer that says it is partial.
    const result = await loadJournalRecords([
      source('okx', 'a', async () => [record({ exchange: 'okx' as ExchangeName, account: 'a' })]),
      source('bybit', 'b', async () => {
        throw new Error('network timeout');
      }),
      source('binance', 'c', async () => [record({ exchange: 'binance' as ExchangeName, account: 'c' })]),
    ]);
    expect(result.records).toHaveLength(2);
    expect(result.failed).toHaveLength(1);
  });

  it('never FETCHES an unsupported venue, and still reports it as excluded', async () => {
    // Fetching it costs a request and can only produce a failure, which would turn a known
    // exclusion into a spurious error — two different states for one cause.
    let called = false;
    const result = await loadJournalRecords([
      source('phemex', 'p', async () => {
        called = true;
        return [];
      }),
    ]);

    expect(called).toBe(false);
    expect(result.failed).toHaveLength(0);
    expect(result.excluded).toEqual([expect.objectContaining({ exchange: 'phemex', account: 'p' })]);
    // An exclusion is a known state, not an incomplete load.
    expect(result.complete).toBe(true);
  });

  it('keeps failure and exclusion as DIFFERENT states for the same account list', async () => {
    const result = await loadJournalRecords([
      source('phemex', 'p', async () => []),
      source('okx', 'o', async () => {
        throw new Error('429 too many requests');
      }),
    ]);
    expect(result.excluded.map((e) => e.exchange)).toEqual(['phemex']);
    expect(result.failed.map((f) => f.exchange)).toEqual(['okx']);
  });
});

describe('classifyJournalFailure', () => {
  it.each([
    ['401 unauthorized', 'auth', false],
    ['Invalid signature [INVALID_CLIENT_REQUEST]', 'auth', false],
    ['429 Too Many Requests', 'rate-limit', true],
    ['request timed out', 'network', true],
    ['something nobody has seen', 'unknown', true],
  ])('classifies %s', (message, kind, retryable) => {
    expect(classifyJournalFailure(new Error(message))).toEqual({ kind, retryable });
  });

  it('does not mark an auth failure retryable — retrying an expired key hides the real fix', () => {
    expect(classifyJournalFailure(new Error('api key expired')).retryable).toBe(false);
  });

  it('falls back to unknown rather than guessing, and unknown stays retryable', () => {
    // An over-eager `auth` match would tell a rate-limited user to re-authenticate, which is
    // worse than admitting we do not know.
    expect(classifyJournalFailure(undefined)).toEqual({ kind: 'unknown', retryable: true });
  });
});
