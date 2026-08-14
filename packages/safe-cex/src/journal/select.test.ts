import { describe, expect, it } from 'vitest';

import type { ExchangeName } from '../types';
import type { JournalRecord } from './select';

import { PositionSide } from '../types';

import { selectJournalRecords } from './select';

/**
 * The invariant this file defends: AN EXCLUSION MUST NEVER LOOK LIKE AN ABSENCE.
 *
 * A dropped venue renders an empty journal, and an empty journal reads as "you have no
 * trades" — a claim about the user's trading rather than about our coverage. Same failure as
 * the audit tool printing `rows: 0` for an envelope no reader understood.
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

describe('selectJournalRecords', () => {
  it('withholds an unsupported venue and REPORTS it rather than filtering silently', () => {
    const result = selectJournalRecords([
      record({ exchange: 'okx' as ExchangeName }),
      record({ exchange: 'phemex' as ExchangeName, account: 'phemex-main' }),
    ]);

    expect(result.records).toHaveLength(1);
    expect(result.excluded).toHaveLength(1);
    expect(result.excluded[0]).toMatchObject({ exchange: 'phemex', account: 'phemex-main', recordCount: 1 });
    expect(result.excluded[0].reason).toBeTruthy();
  });

  it('reports a connected unsupported account with ZERO records', () => {
    // The case where silence is most misleading: the page is completely empty, so without
    // this entry nothing on screen explains why.
    const result = selectJournalRecords([], {
      knownAccounts: [{ exchange: 'phemex' as ExchangeName, account: 'phemex-main' }],
    });

    expect(result.records).toHaveLength(0);
    expect(result.excluded).toEqual([
      expect.objectContaining({ exchange: 'phemex', account: 'phemex-main', recordCount: 0 }),
    ]);
  });

  it('does not report a SUPPORTED account that simply has no records', () => {
    // "No trades on okx this month" is a real answer about the user, not about coverage.
    const result = selectJournalRecords([], {
      knownAccounts: [{ exchange: 'okx' as ExchangeName, account: 'main' }],
    });
    expect(result.excluded).toHaveLength(0);
  });

  it('keeps ungraded venues — they are shown badged, never withheld', () => {
    // Withholding paradex/woo/hyperliquid would make the journal wrong by omission for
    // anyone trading them. The grading model badges; it does not hide.
    const result = selectJournalRecords([
      record({ exchange: 'paradex' as ExchangeName }),
      record({ exchange: 'woo' as ExchangeName }),
      record({ exchange: 'hyperliquid' as ExchangeName }),
    ]);
    expect(result.records).toHaveLength(3);
    expect(result.excluded).toHaveLength(0);
  });

  it('applies the window BEFORE the support check so exclusion counts are not inflated', () => {
    // Otherwise a user reads "2 phemex trades hidden" and hunts for two trades that were
    // simply outside the dates they asked for.
    const result = selectJournalRecords(
      [
        record({ exchange: 'phemex' as ExchangeName, closedAt: 500 }),
        record({ exchange: 'phemex' as ExchangeName, closedAt: 5_000 }),
      ],
      { from: 4_000 },
    );
    expect(result.excluded[0]?.recordCount).toBe(1);
  });

  it('counts every withheld record for one account rather than reporting it once', () => {
    const result = selectJournalRecords([
      record({ exchange: 'phemex' as ExchangeName }),
      record({ exchange: 'phemex' as ExchangeName }),
      record({ exchange: 'phemex' as ExchangeName }),
    ]);
    expect(result.excluded).toHaveLength(1);
    expect(result.excluded[0].recordCount).toBe(3);
  });

  it('filters by account and symbol without touching exclusion reporting', () => {
    const result = selectJournalRecords(
      [
        record({ exchange: 'okx' as ExchangeName, account: 'a', symbol: 'BTCUSDT' }),
        record({ exchange: 'okx' as ExchangeName, account: 'b', symbol: 'BTCUSDT' }),
        record({ exchange: 'okx' as ExchangeName, account: 'a', symbol: 'ETHUSDT' }),
      ],
      { accounts: ['a'], symbols: ['BTCUSDT'] },
    );
    expect(result.records).toHaveLength(1);
  });

  it('returns newest first — a journal is read backwards from the last trade', () => {
    const result = selectJournalRecords([
      record({ exchange: 'okx' as ExchangeName, closedAt: 1_000 }),
      record({ exchange: 'okx' as ExchangeName, closedAt: 3_000 }),
      record({ exchange: 'okx' as ExchangeName, closedAt: 2_000 }),
    ]);
    expect(result.records.map((r) => r.closedAt)).toEqual([3_000, 2_000, 1_000]);
  });

  it('drops a record with no closedAt from a windowed query rather than assuming it fits', () => {
    const result = selectJournalRecords([record({ exchange: 'okx' as ExchangeName, closedAt: undefined })], {
      from: 1_000,
    });
    expect(result.records).toHaveLength(0);
  });
});
