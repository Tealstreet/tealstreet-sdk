import type { ExchangeName } from '../types';
import type { PositionRecord } from './position.types';

import { getJournalVenueStatus } from './venue-grading';

/**
 * Choosing which journal records to render, and REPORTING what was left out.
 *
 * **The whole design constraint is that an exclusion must never look like an absence.** A
 * phemex account whose records are dropped renders an empty journal, and an empty journal
 * reads as "you have no trades" — which is a statement about the user's trading rather than
 * about our coverage. That is the same failure as the audit tool printing `rows: 0` for a
 * response no reader understood, and as the legacy writer emitting `exitPrice: 0` when it
 * did not observe a close.
 *
 * So this returns exclusions as DATA rather than filtering them away, and the caller is
 * expected to surface them. A silent filter here would undo the entire grading model one
 * layer above it.
 */

export type JournalRecord = PositionRecord & {
  readonly exchange: ExchangeName;
  readonly account: string;
};

export type JournalExclusion = {
  readonly exchange: ExchangeName;
  readonly account: string;
  /** How many records were withheld. Zero is meaningful: the venue is unsupported AND idle. */
  readonly recordCount: number;
  /** User-facing, from the venue grading model. */
  readonly reason: string;
};

export type JournalSelection = {
  readonly records: readonly JournalRecord[];
  /**
   * Venues present in the input (or in `knownAccounts`) that cannot be rendered. NON-EMPTY IS
   * NORMAL and must be shown — an unsupported venue the user actually trades on is exactly
   * the case where silence is most misleading.
   */
  readonly excluded: readonly JournalExclusion[];
};

export type JournalSelectionOptions = {
  /** Inclusive lower bound on `closedAt`, ms. */
  readonly from?: number;
  /** Inclusive upper bound on `closedAt`, ms. */
  readonly to?: number;
  readonly symbols?: readonly string[];
  readonly accounts?: readonly string[];
  /**
   * Accounts the user has connected, even ones with no records in this window.
   *
   * Without this an unsupported venue is invisible whenever it happens to have produced
   * nothing — which is precisely when the user most needs telling, because the page they are
   * looking at is empty and they will read that as their own history.
   */
  readonly knownAccounts?: ReadonlyArray<{ readonly exchange: ExchangeName; readonly account: string }>;
};

export const selectJournalRecords = (
  input: readonly JournalRecord[],
  options: JournalSelectionOptions = {},
): JournalSelection => {
  const accountFilter = options.accounts?.length ? new Set(options.accounts) : undefined;
  const symbolFilter = options.symbols?.length ? new Set(options.symbols) : undefined;

  const records: JournalRecord[] = [];
  const withheld = new Map<string, JournalExclusion>();

  const withhold = (exchange: ExchangeName, account: string) => {
    const key = `${exchange}|${account}`;
    const existing = withheld.get(key);
    withheld.set(key, {
      exchange,
      account,
      recordCount: (existing?.recordCount ?? 0) + 1,
      reason: existing?.reason ?? getJournalVenueStatus(exchange).reason ?? 'Not available for this exchange yet.',
    });
  };

  for (const record of input) {
    if (accountFilter && !accountFilter.has(record.account)) continue;
    if (symbolFilter && !symbolFilter.has(record.symbol)) continue;

    // Window filtering happens BEFORE the support check so an out-of-window record does not
    // inflate an exclusion count — the user would read "12 phemex trades hidden" and go
    // looking for twelve trades that are simply outside the dates they asked for.
    const closedAt = record.closedAt;
    if (options.from !== undefined && (closedAt === undefined || closedAt < options.from)) continue;
    if (options.to !== undefined && (closedAt === undefined || closedAt > options.to)) continue;

    if (getJournalVenueStatus(record.exchange).grade === 'unsupported') {
      withhold(record.exchange, record.account);
      continue;
    }

    records.push(record);
  }

  // Connected-but-unsupported accounts with nothing in the window still get an entry, at
  // count 0. Otherwise the one case where the page is completely empty is also the one case
  // where nothing explains why.
  for (const known of options.knownAccounts ?? []) {
    if (accountFilter && !accountFilter.has(known.account)) continue;
    if (getJournalVenueStatus(known.exchange).grade !== 'unsupported') continue;

    const key = `${known.exchange}|${known.account}`;
    if (withheld.has(key)) continue;
    withheld.set(key, {
      exchange: known.exchange,
      account: known.account,
      recordCount: 0,
      reason: getJournalVenueStatus(known.exchange).reason ?? 'Not available for this exchange yet.',
    });
  }

  // Newest first: a journal is read from the most recent trade backwards.
  records.sort((a, b) => (b.closedAt ?? 0) - (a.closedAt ?? 0));

  return { records, excluded: [...withheld.values()] };
};
