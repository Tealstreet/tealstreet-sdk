import type { Execution } from '../types';

import { PositionSide } from '../types';

/**
 * Reconstruct the interval each position was open, from the fills stream.
 *
 * WHY THIS EXISTS. Two things in the canonical model are blocked on one missing input:
 *
 * 1. **`openedAt`** — bybit's `/v5/position/closed-pnl` reports `opened == closed` on every
 *    row, so hold time is not derivable from the axis that reports the close.
 * 2. **Funding attribution** — bybit's `closedPnl` is net of funding but carries no funding
 *    field, and funding can only be attributed to a round trip if you know the interval the
 *    position was open. Measured: a 30-day window missed the ledger by exactly its funding.
 *
 * Both need the same thing, so this produces it once.
 *
 * THE FOLD IS PATH-DEPENDENT AND THAT IS NOT HIDDEN. Running a signed size across fills
 * means one missing or duplicated fill corrupts every interval after it — unlike a
 * venue-reported row, whose error stays local. Callers must treat the output as `derived`
 * and must check `complete`: a window that starts mid-position cannot know the true open
 * time, and reporting the window's first fill as the open would understate every hold time
 * on the boundary.
 *
 * **THE SIDE-INVERSION HAZARD — the worst failure this function has.** If the window opens
 * mid-position and `openingSize` is NOT supplied, the closing fills of that position look
 * exactly like fills OPENING the opposite side. Three sells against an invisible long are
 * indistinguishable from three sells opening a short. If a later fill then reverses it, the
 * fold emits a complete, confident round trip **on the wrong side**, with a wrong entry
 * price and a wrong PnL sign.
 *
 * Measured on binance (sheet row 2): `STGUSDT` had 3 fills and +95.87 of ledger realized
 * PnL over 90 days, from a position opened before the fills in view. The fold read those
 * closes as opening a short, which never closed, so the record was dropped — the identity
 * came up short by the full 95.87 rather than reporting a reversed position. That is the
 * lucky version of this bug.
 *
 * There is no way to detect it from the fills alone; the signal is external — a venue
 * ledger showing realized PnL for a symbol the fold produced no closed position for. Supply
 * `openingSize` whenever the caller can, and treat an unexplained ledger-only symbol as
 * proof the window is too narrow, not as a mapping defect.
 *
 * **AND THAT DETECTOR ONLY WORKS WHERE THE LEDGER IS INDEPENDENT.** On a venue whose ledger
 * is itself derived from the same fills — hyperliquid, where both axes read
 * `userFillsByTime` — a position invisible to the fold is equally invisible to the ledger,
 * so the two agree and nothing fires. The hazard is not handled there; it is merely
 * unobservable. Do not read "the gate is green" on such a venue as "the fold is correct".
 */

/** An epsilon, not `=== 0`: sizes are parsed decimals and a flat position lands on dust. */
const FLAT_EPSILON = 1e-9;

export type PositionInterval = {
  readonly symbol: string;
  readonly positionSide: PositionSide | 'both';
  /** Direction of the position while it was open. */
  readonly side: PositionSide;
  /** MILLISECONDS. `Execution.timestamp` is seconds; converted here once. */
  readonly openedAt: number;
  /** Absent while the position is still open at the end of the fills window. */
  readonly closedAt?: number;
  /** Peak absolute size the position reached. */
  readonly maxSize: number;
  /** Total volume closed over the interval. */
  readonly closedSize: number;
  /**
   * False when the fills window began with this position ALREADY open, so its true open
   * time is older than any fill we can see. The interval is still emitted — its close is
   * real — but `openedAt` is a floor, not the answer.
   */
  readonly complete: boolean;
  /** True when a single fill crossed zero, closing this position and opening the next. */
  readonly closedByFlip: boolean;

  /**
   * Size-weighted average price of the fills that BUILT the position, and of those that
   * closed it.
   *
   * Only derivable here, from the fills. `deriveBinanceClosedPositions` sets
   * `entryPrice: 0` precisely because a single closing fill cannot know it — and a
   * plausible wrong entry silently corrupts every return percentage downstream, which is
   * worse than an obvious zero. Carrying running state is what makes the real number
   * available.
   *
   * On an INCOMPLETE interval the entry covers only the fills in view, so it is a partial
   * average, not the position's true entry.
   */
  readonly avgEntryPrice: number;
  readonly avgExitPrice: number;

  /**
   * Venue-reported realized PnL summed over the closing fills, when the venue puts it on
   * the fill (binance, okx, blofin, hyperliquid, paradex). `undefined` where no fill in the
   * interval reported one — distinguishing "the venue said zero" from "the venue said
   * nothing", which a 0 default would erase.
   */
  readonly realizedPnl?: number;
  /**
   * True when SOME fills carried venue PnL and others did not, so no total can be formed.
   * Distinct from `realizedPnl === undefined` with this false, which means the venue reported
   * none at all — a venue limitation rather than a windowing artifact.
   */
  readonly realizedPnlPartial?: boolean;
  /** Commission summed over every fill in the interval, positive = cost. */
  readonly fees: number;
  /**
   * Every distinct commission asset seen. Binance can charge fees in BNB while the position
   * settles in USDT, so a single summed `fees` number is only meaningful when this holds one
   * currency — otherwise it is adding different units together.
   */
  readonly feeCurrencies: string[];
};

export type PositionIntervalOptions = {
  /**
   * Size the position was already carrying when the window opens, signed (+long/-short),
   * per `symbol|leg`. Supplying it turns an incomplete boundary interval into a complete
   * one only if the caller ALSO knows when it opened — which is why there is no
   * `openedAt` input here and boundary intervals stay `complete: false`.
   */
  readonly openingSize?: Record<string, number>;
  /** Leg for a fill that does not report one, e.g. from an order-axis join on `orderId`. */
  readonly resolveLeg?: (execution: Execution) => PositionSide | 'both' | undefined;
};

type RunningPosition = {
  signed: number;
  openedAt: number;
  maxSize: number;
  closedSize: number;
  complete: boolean;
  entryQty: number;
  entryNotional: number;
  exitQty: number;
  exitNotional: number;
  realizedPnl: number;
  sawRealizedPnl: boolean;
  sawMissingRealizedPnl: boolean;
  fees: number;
  feeCurrencies: Set<string>;
};

const freshPosition = (over: Partial<RunningPosition> = {}): RunningPosition => ({
  signed: 0,
  openedAt: 0,
  maxSize: 0,
  closedSize: 0,
  complete: true,
  entryQty: 0,
  entryNotional: 0,
  exitQty: 0,
  exitNotional: 0,
  realizedPnl: 0,
  sawRealizedPnl: false,
  sawMissingRealizedPnl: false,
  fees: 0,
  feeCurrencies: new Set<string>(),
  ...over,
});

/** Fees and venue-reported PnL accrue on EVERY fill, opening or closing. */
const accrueFill = (position: RunningPosition, execution: Execution, reduces = false) => {
  if (execution.commission !== undefined) {
    position.fees += execution.commission;
    if (execution.commissionAsset) position.feeCurrencies.add(execution.commissionAsset);
  }
  if (execution.realizedPnl !== undefined) {
    position.realizedPnl += execution.realizedPnl;
    position.sawRealizedPnl = true;
  } else if (reduces) {
    // A REDUCING fill the venue reported no PnL for. Only reducing fills matter: an opening
    // fill realizes nothing by definition, and many venues simply omit the field on one —
    // treating that as a gap marked every ordinary round trip unreportable, which the
    // existing interval tests caught immediately.
    //
    // An interval mixing reported and unreported CLOSES is the dangerous case:
    // `sawRealizedPnl` alone is true, so it reports a sum that silently omits this fill and
    // looks complete.
    //
    // Reachable for the first time on kraken. Its `/v3/fills` drops `realized_pnl` entirely
    // once `lastFillTime` is supplied — the venue offers windowing OR PnL, never both — so a
    // position whose closes straddle the newest-100 boundary has some closes carrying PnL
    // and some not. Every other venue is uniform: binance and bitget report it on every
    // fill, woo on none (which the all-undefined refusal already catches).
    position.sawMissingRealizedPnl = true;
  }
};

const finish = (
  position: RunningPosition,
  symbol: string,
  leg: PositionSide | 'both',
  closedAt?: number,
): PositionInterval => ({
  symbol,
  positionSide: leg,
  side: position.signed > 0 ? PositionSide.Long : PositionSide.Short,
  openedAt: position.openedAt,
  closedAt,
  maxSize: position.maxSize,
  closedSize: position.closedSize,
  complete: position.complete,
  closedByFlip: false,
  avgEntryPrice: position.entryQty > 0 ? position.entryNotional / position.entryQty : 0,
  avgExitPrice: position.exitQty > 0 ? position.exitNotional / position.exitQty : 0,
  // UNDEFINED when the population is mixed, not just when it is empty. A partial sum
  // presented as a total is worse than no number: it reconciles nowhere, and the residual it
  // creates reads as a mapping defect rather than as missing data.
  realizedPnl: position.sawRealizedPnl && !position.sawMissingRealizedPnl ? position.realizedPnl : undefined,
  realizedPnlPartial: position.sawRealizedPnl && position.sawMissingRealizedPnl,
  fees: position.fees,
  feeCurrencies: [...position.feeCurrencies],
});

const legKey = (symbol: string, leg: PositionSide | 'both') => `${symbol}|${leg}`;

/**
 * Fills MUST be complete over the window for the result to mean anything. A venue that
 * caps fills paging silently (bybit's executions walk stops at 5 pages per window) will
 * produce a running size that never returns to zero, and every subsequent interval on that
 * symbol is wrong. Callers detect this as intervals that never close.
 */
export const reconstructPositionIntervals = (
  executions: Execution[],
  options: PositionIntervalOptions = {},
): PositionInterval[] => {
  const ordered = [...executions].sort((a, b) => a.timestamp - b.timestamp);
  const running = new Map<string, RunningPosition>();
  const intervals: PositionInterval[] = [];

  // Seed positions the window opened with. Their true open time is unknowable from these
  // fills, so they are marked incomplete and their `openedAt` is the first fill we see.
  for (const [key, signed] of Object.entries(options.openingSize ?? {})) {
    if (Math.abs(signed) <= FLAT_EPSILON) continue;
    running.set(key, freshPosition({ signed, maxSize: Math.abs(signed), complete: false }));
  }

  for (const execution of ordered) {
    const leg = execution.positionSide ?? options.resolveLeg?.(execution) ?? 'both';
    const key = legKey(execution.symbol, leg);
    // `Execution.timestamp` is SECONDS by contract; the model works in ms.
    const at = execution.timestamp * 1000;
    const delta = execution.side === 'buy' ? execution.qty : -execution.qty;
    const price = execution.price;

    const current = running.get(key);

    if (!current || Math.abs(current.signed) <= FLAT_EPSILON) {
      // Opening from flat.
      const opened = freshPosition({
        signed: delta,
        openedAt: current && !current.complete ? current.openedAt || at : at,
        maxSize: Math.abs(delta),
        complete: current ? current.complete : true,
        entryQty: execution.qty,
        entryNotional: execution.qty * price,
      });
      accrueFill(opened, execution);
      running.set(key, opened);
      continue;
    }

    if (current.openedAt === 0) current.openedAt = at;

    const next = current.signed + delta;
    const sameDirection = Math.sign(delta) === Math.sign(current.signed);

    if (sameDirection) {
      current.signed = next;
      current.maxSize = Math.max(current.maxSize, Math.abs(next));
      current.entryQty += execution.qty;
      current.entryNotional += execution.qty * price;
      accrueFill(current, execution);
      continue;
    }

    // Reducing, closing, or crossing through zero.
    const closedNow = Math.min(Math.abs(delta), Math.abs(current.signed));
    current.closedSize += closedNow;
    current.exitQty += closedNow;
    current.exitNotional += closedNow * price;

    if (Math.abs(next) > FLAT_EPSILON && Math.sign(next) === Math.sign(current.signed)) {
      // Partial close — the position survives.
      current.signed = next;
      accrueFill(current, execution, true);
      continue;
    }

    // Flat, or crossed to the opposite side. A CROSS IS A CLOSE PLUS AN OPEN, and the
    // venue reports it as ONE fill with ONE realized PnL and no marker for where the close
    // ends. It is derivable only from the size held BEFORE the fill, which is exactly the
    // running state this function carries — measured on a live account as a sell of 283
    // against a 141 long, leaving 142 short.
    const crossed = Math.abs(next) > FLAT_EPSILON;

    if (!crossed) {
      accrueFill(current, execution, true);
      intervals.push({ ...finish(current, execution.symbol, leg, at), closedByFlip: false });
      running.delete(key);
      continue;
    }

    // On a CROSS the fee is charged on the whole fill but only `closedNow` of it closed the
    // old position, so it is split by size. Realized PnL is NOT split — it is realized
    // entirely by the closing portion; the opening portion cannot have realized anything.
    const closedShare = closedNow / Math.abs(delta);
    accrueFill(
      current,
      {
        ...execution,
        commission: execution.commission === undefined ? undefined : execution.commission * closedShare,
      },
      true,
    );
    intervals.push({ ...finish(current, execution.symbol, leg, at), closedByFlip: true });

    const openedSize = Math.abs(next);
    const opened = freshPosition({
      signed: next,
      openedAt: at,
      maxSize: openedSize,
      entryQty: openedSize,
      entryNotional: openedSize * price,
      fees: execution.commission === undefined ? 0 : execution.commission * (1 - closedShare),
      feeCurrencies: new Set(execution.commissionAsset ? [execution.commissionAsset] : []),
    });
    running.set(key, opened);
  }

  // Positions still open when the window ends are emitted WITHOUT a `closedAt` rather than
  // dropped. Dropping them would make an open position indistinguishable from one that
  // never existed, and the caller needs to know a symbol is still carrying size.
  for (const [key, current] of running.entries()) {
    if (Math.abs(current.signed) <= FLAT_EPSILON) continue;
    const [symbol, leg] = [key.slice(0, key.lastIndexOf('|')), key.slice(key.lastIndexOf('|') + 1)];
    intervals.push(finish(current, symbol, leg as PositionSide | 'both', undefined));
  }

  return intervals.sort((a, b) => (a.closedAt ?? Number.MAX_SAFE_INTEGER) - (b.closedAt ?? Number.MAX_SAFE_INTEGER));
};

/**
 * Sum signed funding for a symbol over the interval a position was open.
 *
 * `fundingAttributed` is FALSE whenever the attribution is not exact, and the caller must
 * propagate that rather than presenting the number as venue-reported:
 *
 * - **Hedge mode**: funding arrives per SYMBOL, not per leg. With both legs open there is
 *   no venue-reported split, and any allocation we invent is a fabrication.
 * - **Incomplete interval**: a position already open when the window began has an unknown
 *   start, so funding before the first visible fill is missed.
 * - **Still open**: no close time, so the interval is open-ended.
 */
export const attributeFunding = (
  interval: PositionInterval,
  fundingRows: Array<{ symbol: string; timestamp: number; income: number }>,
  opts: { hedged: boolean },
): { funding: number; attributed: boolean } => {
  if (interval.closedAt === undefined || !interval.complete) {
    return { funding: 0, attributed: false };
  }

  let funding = 0;
  for (const row of fundingRows) {
    if (row.symbol !== interval.symbol) continue;
    if (row.timestamp < interval.openedAt || row.timestamp > interval.closedAt) continue;
    funding += row.income;
  }

  // A hedged symbol's funding cannot be split between the legs, so the SUM is reported but
  // flagged unattributed — the number is right for the symbol and wrong for the leg.
  return { funding, attributed: !opts.hedged };
};
