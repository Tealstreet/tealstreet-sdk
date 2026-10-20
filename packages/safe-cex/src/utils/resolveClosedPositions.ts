import type { ClosedPnlEntry, DataProvenance, HistoricalTradeOptions } from '../types';

/**
 * Closed positions, however the venue can supply them, plus what that supply is worth.
 *
 * **Why this is a resolver and not a venue method.** `fetchClosedPnl` means exactly one
 * thing: the exchange reported this. Venues without a closed-position endpoint can still
 * produce the axis by folding their own fills — but putting that fold behind the same
 * method name launders derived data through a name that implies it was reported, and no
 * caller can tell afterwards. Composing here keeps the venue method honest and gives
 * polymorphic consumers a single call.
 *
 * It also removes the duplicate-fetch problem. Two sibling methods that both fold fills
 * fetch fills twice; one resolver fetches once and returns both views.
 *
 * Consumers should branch on `provenance`, not on whether a method exists:
 *
 * - A journal renders either, but must not present `derived` rows as verified, because
 *   their error is path-dependent — one bad fill corrupts every position after it.
 * - A validator (the `--closed-pnl` oracle, a runtime health check) requires
 *   `independentlyVerifiable`. Cross-checking a ledger against positions folded from the
 *   SAME source compares data to itself and always agrees. Note this is about the source,
 *   not about being a fold: a fold of a different endpoint is a legitimate check.
 * - Anything rendering a "trade count" should know `derived` granularity is per closing
 *   FILL, while venue-reported is usually per closed POSITION.
 */
export type ClosedPositionsResult = {
  entries: ClosedPnlEntry[];
  provenance: DataProvenance;
  /**
   * True only when these entries were observed independently of the ledger axis, so they
   * can be used to CHECK it. False for anything folded from fills.
   */
  independentlyVerifiable: boolean;
  /** Set when neither path was available, so an empty list is not mistaken for "no trades". */
  unavailableReason?: string;
};

/** The minimum surface this needs, kept structural to avoid importing the Exchange type. */
type ClosedPositionSource = {
  fetchClosedPnl: (opts: HistoricalTradeOptions) => Promise<ClosedPnlEntry[]>;
  deriveClosedPositions?: (opts: HistoricalTradeOptions) => Promise<ClosedPnlEntry[]>;
  /**
   * Whether `deriveClosedPositions` folds the SAME rows the ledger is built from.
   *
   * Provenance and independence are different questions, and conflating them was wrong.
   * `derived` describes how a number was produced; independence describes whether its
   * SOURCE differs from the axis being checked. Hyperliquid folds `userFillsByTime`, which
   * is exactly what its ledger reads — so it cannot check it. Binance folds
   * `/fapi/v1/userTrades`, a different endpoint from the `/fapi/v1/income` its ledger
   * reads — so it CAN, even though it is a fold.
   *
   * Defaults to `true` (assume not independent) because that is the safe direction: a
   * venue that forgets to declare gets a SKIP, not a meaningless PASS.
   */
  closedPositionsShareLedgerSource?: boolean;
};

export const resolveClosedPositions = async (
  exchange: ClosedPositionSource,
  opts: HistoricalTradeOptions,
  /**
   * Pass `!exchange.capabilities.closedPnl`. Injected
   * rather than imported so this utility does not depend on `exchanges/base`.
   */
  isVenueReportedUnavailable: boolean,
): Promise<ClosedPositionsResult> => {
  if (!isVenueReportedUnavailable) {
    // `provenance` is required on the type, so a venue producer has already declared it;
    // this does not overwrite a producer that knows better.
    const entries = await exchange.fetchClosedPnl(opts);
    return { entries, provenance: 'venue', independentlyVerifiable: true };
  }

  if (exchange.deriveClosedPositions) {
    const entries = await exchange.deriveClosedPositions(opts);
    const sharesSource = exchange.closedPositionsShareLedgerSource ?? true;
    return {
      entries: entries.map((entry) => ({ ...entry, provenance: 'derived' as const })),
      provenance: 'derived',
      // A fold can still be an independent check — if it folds a DIFFERENT source than
      // the ledger reads. What disqualifies it is sharing the source, not being a fold.
      independentlyVerifiable: !sharesSource,
    };
  }

  return {
    entries: [],
    provenance: 'derived',
    independentlyVerifiable: false,
    unavailableReason: 'venue reports no closed positions and no derivation is implemented',
  };
};
