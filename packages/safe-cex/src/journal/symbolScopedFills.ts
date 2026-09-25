import { isBaseCapabilityFallback } from '../exchanges/base';

import type { ExchangeName } from '../types';

/**
 * Venues whose `fetchHistoricalFills` REFUSES an account-wide ask.
 *
 * Their implementations open with `if (!symbol) return [];` — no request, no
 * error, an empty array. That is indistinguishable from "this account had no
 * fills", and it is the single most dangerous shape in this subsystem: ingestion
 * asked account-wide, got `[]`, recorded the window as covered, and advanced the
 * watermark over data it never requested. Caught on preview 2026-08-17 with five
 * accounts reporting perfect health and zero rows.
 *
 * **A venue listed here can only be collected symbol by symbol.** The symbol list
 * comes from the income ledger — what the account ACTUALLY traded — exactly as
 * `buildFoldedPositionRecords` already does for the positions axis. Guessing a
 * list measures a subset, and a subset looks like a quiet account rather than a
 * partial answer.
 *
 * **This list is verified against the source by `symbolScopedFills.test.ts`.** It
 * is not a hand-maintained opinion: the test scans every `*.exchange.ts` for the
 * guard and fails if the registry disagrees. A hand list would drift the first
 * time someone added or removed the early return, and the drift would present as
 * silent under-collection rather than as a broken build — which is how the
 * `positions` axis ended up declaring only three of these symbol-scoped venues.
 */
export const SYMBOL_SCOPED_FILLS: ReadonlySet<ExchangeName> = new Set([
  'apexomni',
  'asterdex',
  'binance',
  'bingx',
  'bitunix',
  'blofin',
  'bybit',
  'bybitinverse',
  'hyperliquid',
  'kucoinspot',
  'lighter',
  'lighter-robinhood',
  'mexc',
  'okx',
  'paradex',
  'phemex',
] as ExchangeName[]);

export const isSymbolScopedFills = (exchange: ExchangeName): boolean => SYMBOL_SCOPED_FILLS.has(exchange);

/**
 * Whether the traded-symbol list can be discovered at all.
 *
 * A symbol-scoped venue with no income ledger cannot be collected account-wide
 * NOR symbol by symbol, because there is nothing to enumerate from. The correct
 * report for those is `unsupported` — "we have no way to ask" — and never an
 * empty result. EIGHT venues sit here today — apexomni, asterdex, bingx,
 * kucoinspot, lighter, lighter-robinhood, mexc, phemex — each needing either an
 * income route or a venue-specific symbol source before its fills can be held.
 * `bybitinverse` left that list when it got a real transaction-log route. The
 * count was wrong in both directions before that: it said seven, named
 * bybitinverse, and omitted mexc and lighter-robinhood. Derive it rather than
 * trusting it — a venue class with no `fetchIncomeHistoryRange =` of its own
 * carries the base stub.
 *
 * **A branded stub is not a route.** `BaseExchange.fetchIncomeHistoryRange` is a
 * function returning `[]` on EVERY venue, so a bare `typeof === 'function'` check
 * answered `true` for all of them — the exact duck-typing `base.ts` warns about
 * where it defines the stub. The enumerator then asked, received no symbols, and
 * collection closed the window in perfect health having fetched nothing. That is
 * the silent-empty this module exists to prevent, so the brand is checked.
 */
export const canEnumerateFillSymbols = (exchange: { fetchIncomeHistoryRange?: unknown }): boolean =>
  typeof exchange.fetchIncomeHistoryRange === 'function' &&
  !isBaseCapabilityFallback(exchange.fetchIncomeHistoryRange, 'fetchIncomeHistoryRange');
