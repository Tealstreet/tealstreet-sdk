import type { PositionInterval } from './position-intervals';
import type { PositionRecord } from './position.types';

/**
 * A reconstructed interval -> canonical `PositionRecord`, for venues with NO closed-position
 * endpoint. Shared by binance and hyperliquid.
 *
 * **STRUCTURALLY DIFFERENT FROM okx AND bybit.** Both of those report a closed-position
 * row and the adapter converts its conventions. These venues publish no such row at all, so
 * there is nothing to convert — the position is reconstructed from the fills axis by
 * `reconstructPositionIntervals`, and every record is therefore `provenance: 'derived'`.
 *
 * One mapper, not one per venue, because the thing most likely to drift is the PnL
 * convention — and both venues share it: `closedPnl`/`realizedPnl` on the fill is GROSS,
 * with the fee on the same row. Two copies would let one silently acquire a fee adjustment.
 *
 * That distinction is load-bearing and must reach the consumer, not be smoothed away. A
 * fold of fills is PATH-DEPENDENT: one missing or duplicated fill corrupts every position
 * after it on that symbol, whereas a venue-reported row's error stays local. Consumers must
 * not present these as verified the way they can present okx's.
 *
 * **Binance is a THIRD PnL convention**, which is the whole reason the model exists:
 *
 * | Venue        | What the venue's realized-PnL figure already contains |
 * | ------------ | ----------------------------------------------------- |
 * | binance      | gross — `commission` is a separate field on the row    |
 * | hyperliquid  | gross — `closedPnl` with `fee` alongside it            |
 * | bybit        | net of fees AND of whole-life funding                  |
 * | okx          | net of fees AND funding                               |
 *
 * So this adapter passes `realizedPnl` through unmodified while the other two subtract.
 * Getting it wrong shifts every number by the fee term and still reconciles in aggregate.
 */

export type FoldPositionAdapterDeps = {
  account: string;
  /** Settlement currency for a symbol, from the market's quote currency. */
  resolveSettleCurrency: (symbol: string) => string;
  /**
   * Funding accrued over the interval, from the ledger. Neither venue books funding onto
   * the position — binance uses `FUNDING_FEE` rows against the symbol, hyperliquid a
   * separate `userFunding` request — so it is attributed the same way as bybit's, and
   * refuses the same way when the interval is incomplete or the symbol is hedged.
   */
  resolveFunding?: (interval: PositionInterval) => { funding: number; attributed: boolean };
};

export const mapFoldedInterval = (
  interval: PositionInterval,
  deps: FoldPositionAdapterDeps,
): PositionRecord | null => {
  // Still-open positions are not journal entries. They are emitted by the fold so a caller
  // can see a symbol still carries size; they are not round trips.
  if (interval.closedAt === undefined) return null;
  if (!(interval.closedSize > 0)) return null;

  // REFUSE rather than default to zero when the venue reported no PnL on ANY fill in the
  // interval. `?? 0` would assert "this round trip broke even", which is a fabrication, and
  // on a venue with no ledger there is nothing downstream that could contradict it.
  //
  // Measured on woo: `realizedPnl` is a FIELD on its fills and is `null` on 534 of 534 rows.
  // With no income axis and no closed-position axis either, every woo record would have
  // carried a silent, unfalsifiable zero. `undefined` means the venue said nothing; a real
  // zero from a venue that does report (binance writes 0 on opening fills) still passes.
  if (interval.realizedPnl === undefined) return null;

  const funding = deps.resolveFunding?.(interval);

  return {
    // The fold can produce several round trips per symbol+leg in one window, and a flip
    // makes two of them share an instant, so the close time alone is not an identity.
    key: `${deps.account}|${interval.symbol}|${interval.positionSide}|${interval.openedAt}|${interval.closedAt}`,
    account: deps.account,
    symbol: interval.symbol,
    settleCurrency: deps.resolveSettleCurrency(interval.symbol),
    side: interval.side,
    positionSide: interval.positionSide,
    state: 'closed',
    // A REAL open time, unlike bybit's axis — the fold carries it. Absent only when the
    // position was already open before the fills in view, where the first visible fill
    // would be a floor rather than the answer.
    openedAt: interval.complete ? interval.openedAt : undefined,
    closedAt: interval.closedAt,
    closedSize: interval.closedSize,
    maxSize: interval.maxSize,
    avgEntryPrice: interval.avgEntryPrice,
    avgExitPrice: interval.avgExitPrice || undefined,
    // GROSS already on this venue — passed through, NOT adjusted. `commission` is a
    // separate field, so adding it back (as bybit needs) would double-count the fee.
    realizedPnl: interval.realizedPnl,
    fees: interval.fees,
    funding: funding?.funding ?? 0,
    fundingAttributed: funding?.attributed ?? false,
    // Neither venue reports a close reason on a fill. On binance a liquidation appears in
    // the ledger as its own income type, not on the trade; hyperliquid puts it in
    // `userNonFundingLedgerUpdates`, which is not wired. So this axis genuinely cannot
    // distinguish one — left undefined rather than defaulted to 'user', which would assert
    // something untrue.
    closeReason: undefined,
    provenance: 'derived',
    sourceAxis: 'fills',
    // The fold's unit is already the round trip, however many closing fills composed it.
    closeCount: 1,
  };
};
