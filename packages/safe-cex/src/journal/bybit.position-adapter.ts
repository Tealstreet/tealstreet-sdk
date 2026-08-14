import type { PositionRecord } from './position.types';

import { PositionSide } from '../types';

/**
 * Bybit `/v5/position/closed-pnl` -> canonical `PositionRecord`.
 *
 * Unlike OKX, a bybit row is NOT a round trip. Measured on an identical ladder (open 3x,
 * close in thirds): OKX emitted one row for the whole position, bybit emitted three — one
 * per partial close, each carrying the position's entry price and that partial's exit.
 * Left unaggregated, a trade count reads 1 on OKX and 3 on bybit for the same activity,
 * and every per-trade statistic is wrong on one of them.
 *
 * So this folds. Three venue facts constrain how:
 *
 * - **`opened == closed` on every row**, so the fold cannot group by open time and the
 *   resulting record has no `openedAt` — the model makes that field optional precisely
 *   because forcing it here would claim a zero hold time for every bybit trade.
 * - **`closedPnl` is NET** of `openFee` + `closeFee`; canonical is gross.
 * - **`side` is the CLOSING side and inverts** — a `Sell` close means the position was
 *   long. Getting this backwards flips every record and still typechecks.
 */

const num = (value: unknown) => {
  const parsed = Number.parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

/**
 * Bybit reports the manner of the close, so liquidation and ADL are distinguishable here
 * even though neither has been deliberately produced on a test account yet.
 */
const CLOSE_REASON: Record<string, PositionRecord['closeReason']> = {
  Trade: 'user',
  AdlTrade: 'adl',
  BustTrade: 'liquidation',
  Settle: 'settlement',
};

export type BybitPositionAdapterDeps = {
  account: string;
  /**
   * Settlement currency for a symbol, from the market's quote currency.
   *
   * A RESOLVER, not a constant. Bybit's `getCategoryForSymbol` returns `linear` for
   * everything and its linear category spans both USDT and USDC quotes, so one closed-PnL
   * response legitimately mixes settlement currencies. Stamping a single currency across
   * the batch would mislabel every USDC position AND make a multi-currency check pass by
   * construction — the row itself carries no currency to catch it with.
   */
  resolveSettleCurrency: (symbol: string) => string;
  /**
   * The hedge leg for a closing order, from `/v5/order/history.positionIdx`
   * (`0` one-way, `1` long, `2` short).
   *
   * Required as a dependency rather than read off the row because bybit does not put the
   * leg on the closed-PnL row OR on the fill — it exists only on the order axis, so a
   * hedged account is a two-axis join on `orderId`. Returning `undefined` means the join
   * was unavailable, and the record falls back to `'both'`.
   */
  resolveLeg?: (orderId: string) => PositionSide | 'both' | undefined;
  /**
   * The fills-derived context for a round trip: when it opened, and the funding that
   * accrued while it was open.
   *
   * Supplied rather than read off the row because bybit reports NEITHER. Every closed-PnL
   * row returns `opened == closed`, and `closedPnl` is net of funding while carrying no
   * funding field — so both come from `reconstructPositionIntervals` over the fills axis.
   *
   * Returning `undefined` (no fills available, or the position was already open when the
   * fills window began) leaves the record exactly as it was: no `openedAt`, funding 0, and
   * `realizedPnl` still net of funding. That is the honest degradation — the alternative is
   * a confident number built from an interval we could not actually see.
   */
  resolveContext?: (args: {
    symbol: string;
    positionSide: PositionSide | 'both';
    closedAt: number;
  }) => { openedAt?: number; funding: number; fundingAttributed: boolean } | undefined;
};

/**
 * Fold bybit's per-partial rows into round trips.
 *
 * **The grouping heuristic, and its limits.** Rows are bucketed by
 * `(symbol, leg, side, avgEntryPrice)`. A position's entry price is constant across its
 * partial closes, which is what makes this work — verified on the measured ladder, where all three rows carried `entry=0.07088`.
 *
 * It is a heuristic, not a proof, and it is wrong in two knowable ways:
 *
 * 1. Closing a position fully and re-opening it at the *identical* average price merges
 *    two round trips into one.
 * 2. Adding to a position between partial closes changes its average entry, which splits
 *    one round trip into two.
 *
 * Both are resolvable only by carrying running position state from fills. Until that
 * exists, `closeCount` makes the fold visible to consumers and these limits are stated
 * rather than hidden behind a confident-looking number.
 */
export const aggregateBybitClosedPnl = (
  rows: Array<Record<string, any>>,
  deps: BybitPositionAdapterDeps,
): PositionRecord[] => {
  // Bucket by key FIRST, then split each bucket into runs. Comparing only against the
  // previously seen row is wrong the moment two symbols or two hedge legs close
  // interleaved: DOGE@t1, BTC@t2, DOGE@t3 reads as three runs and puts bybit straight
  // back onto per-partial grain, which is the single thing this fold exists to prevent.
  const buckets = new Map<string, Array<Record<string, any>>>();
  for (const row of rows) {
    const leg = deps.resolveLeg?.(String(row.orderId ?? '')) ?? 'both';
    // Compare the entry price NUMERICALLY: bybit returns decimal strings, so "0.07088"
    // and "0.070880" are the same price and must not split a round trip.
    const key = `${row.symbol}|${leg}|${row.side}|${num(row.avgEntryPrice)}`;
    const bucket = buckets.get(key);
    if (bucket) bucket.push(row);
    else buckets.set(key, [row]);
  }

  const records: PositionRecord[] = [];
  for (const bucket of buckets.values()) {
    const ordered = [...bucket].sort((a, b) => num(a.updatedTime) - num(b.updatedTime));
    // Within a bucket every row shares symbol, leg, side and entry price, so the only
    // thing separating two round trips is a re-open at the identical average price.
    //
    // **THAT COLLISION IS NOT HYPOTHETICAL — IT HAS BEEN MEASURED, AND IT LOSES A ROUND
    // TRIP.** This note previously called it a documented limit with "no signal on this
    // axis to split it", which read as a corner case. On the nightly e2e account
    // (2026-08-12) the suite ran four long DOGEUSDT round trips and this fold produced
    // three: two of the flip cycles closed at an IDENTICAL `avgEntryPrice` of 0.07078
    // (rows `updatedTime` 1786547559607 and 1786547721251), so they bucketed together and
    // folded into one record. The third flip escaped only because the market had moved a
    // tick, to 0.07074.
    //
    // Identical entry prices are not rare — they are the NORMAL case for a quiet market
    // and repeated same-size orders, which is exactly what an automated suite and a
    // gridding user both produce. Anything counting trades, win rate or average hold time
    // on bybit is undercounting, and the merged records carry a `closedSize` that is the
    // SUM of two positions.
    //
    // "No signal on this axis" is still true of the closed-PnL rows alone, and is also the
    // wrong frame: the caller reconstructs `PositionInterval[]` from FILLS and already
    // passes them in via `deps.resolveContext`, and those intervals do know where a
    // re-open happened. Splitting a bucket on interval boundaries is the fix; it is not
    // done here because changing the fold on a venue that currently PASSES needs its own
    // verification pass, and this was found by the ground-truth grader
    // (`yarn history-audit --ground-truth`), which can now prove the fix rather than
    // assume it.
    const record = foldRun(ordered, deps);
    if (record) records.push(record);
  }

  return records.sort((a, b) => (a.closedAt ?? 0) - (b.closedAt ?? 0));
};

const foldRun = (run: Array<Record<string, any>>, deps: BybitPositionAdapterDeps): PositionRecord | null => {
  const first = run[0];
  const symbol = String(first.symbol ?? '');
  if (!symbol) return null;

  // `side` is the closing side, so it inverts to give the position's direction.
  const side = first.side === 'Sell' ? PositionSide.Long : PositionSide.Short;
  const leg = deps.resolveLeg?.(String(first.orderId ?? '')) ?? ('both' as const);

  let closedSize = 0;
  let fees = 0;
  let netPnl = 0;
  let exitValue = 0;
  let closedNotional = 0;
  let closedAt = 0;

  for (const row of run) {
    const rowClosed = Math.abs(num(row.closedSize));
    closedSize += rowClosed;
    fees += num(row.openFee) + num(row.closeFee);
    netPnl += num(row.closedPnl);
    exitValue += rowClosed * num(row.avgExitPrice);
    closedNotional += num(row.cumExitValue);
    closedAt = Math.max(closedAt, num(row.updatedTime) || num(row.createdTime));
  }

  if (!(closedSize > 0)) return null;

  // Fills-derived, because bybit reports neither on this axis. Absent context degrades to
  // the previous behaviour rather than inventing a number.
  const context = deps.resolveContext?.({ symbol, positionSide: leg, closedAt });
  const funding = context?.funding ?? 0;
  const fundingAttributed = context?.fundingAttributed ?? false;

  return {
    key: `${deps.account}|${symbol}|${leg}|${first.orderId ?? closedAt}|${closedAt}`,
    account: deps.account,
    symbol,
    settleCurrency: deps.resolveSettleCurrency(symbol),
    side,
    positionSide: leg,
    state: 'closed',
    // Every bybit row reports `opened == closed`, so this can only come from the fills
    // axis. Still absent when fills were unavailable or the position predates the window —
    // claiming the first visible fill would understate every boundary hold time.
    openedAt: context?.openedAt,
    closedAt: closedAt || undefined,
    closedSize,
    // Bybit's closed-PnL rows carry no peak size, and inferring one from the partials
    // would be a guess — so `maxSize` stays absent rather than wrong.
    maxSize: undefined,
    leverage: num(first.leverage) || undefined,
    closedNotional: closedNotional || undefined,
    venueId: first.orderId ? String(first.orderId) : undefined,
    avgEntryPrice: num(first.avgEntryPrice),
    avgExitPrice: exitValue / closedSize,
    // `closedPnl = realized - fees + funding` on this venue, so BOTH terms are removed to
    // reach canonical gross. Measured 2026-08-13: the funding half corrects a `funding:
    // false` that had been assumed, and over 30 days of BTCUSDT the un-subtracted total
    // missed the ledger by -0.05958766 against a window funding of -0.05958766, equal to
    // every digit. No fixture caught it because every fixture had funding 0 — the term was
    // unexercised on both sides at once.
    //
    // `funding` comes from the fills axis via `resolveContext`, never from this row, which
    // carries no funding field. Without that context it stays 0 and this figure remains
    // net-of-funding — which `--canonical` FAILs on rather than absorbing.
    realizedPnl: netPnl + fees - funding,
    fees,
    funding,
    // True only when the interval was fully visible AND the symbol was not hedged; funding
    // arrives per SYMBOL, so with both legs open no venue-reported split exists.
    fundingAttributed,
    closeReason: CLOSE_REASON[String(first.execType ?? '')] ?? 'user',
    provenance: 'reported',
    // Closed-PnL joined to ORDERS for the leg — never fills. The distinction is
    // load-bearing: consumers treat a fills fold as path-dependent and unverifiable,
    // while this stays venue-reported and error-local.
    sourceAxis: deps.resolveLeg ? 'closed-positions+orders' : 'closed-positions',
    closeCount: run.length,
  };
};
