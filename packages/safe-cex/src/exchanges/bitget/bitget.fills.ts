import type { Execution } from '../../types';

import { OrderSide, PositionSide } from '../../types';

/**
 * Bitget `/api/v2/mix/order/fills` -> `Execution`.
 *
 * REPLACES a mapper that read `orders-history` and filtered `state === 'filled'`. That route
 * is ORDER-level, so partial fills collapsed into one row, and it carried none of the fields
 * a position fold needs. The per-fill route measured live (sheet row 36) carries all of them:
 *
 * | field | why it matters |
 * | --- | --- |
 * | `tradeId` | the per-fill id. The old mapper set no `id` at all, so fills could not be deduped |
 * | `profit` | realized PnL per fill — **without this the venue cannot fold records at all** |
 * | `tradeSide` | the hedge leg (`open_long`/`close_short`/…, or `*_single` in one-way mode) |
 * | `posMode` | which spelling `tradeSide` is using |
 * | `feeDetail[].totalFee` | the fee, NEGATIVE when charged, nested in an array |
 * | `tradeScope` | maker/taker |
 *
 * It is also **account-wide** — unlike binance's `userTrades`, no symbol is required, so the
 * audit does not have to discover a symbol list before it can read fills.
 *
 * **`profit` is the fill's own realized PnL and is GROSS**, consistent with the ledger's
 * `amount` (see `bitget.income.ts`, where the running-balance oracle pins that). Fees are
 * reported separately in `feeDetail` and are not deducted from `profit`.
 */

const num = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

/**
 * `feeDetail` is an ARRAY of per-coin fee entries, not a scalar. Summing it is deliberate:
 * a fill can be charged in more than one coin (fee-coin deduction), and reading only `[0]`
 * would silently under-report those.
 */
const totalFeeOf = (row: Record<string, any>): number => {
  const detail = row?.feeDetail;
  if (!Array.isArray(detail)) return num(row?.fee);
  let total = 0;
  for (const entry of detail) total += num(entry?.totalFee);
  return total;
};

/**
 * Hedge leg from `tradeSide`. One-way mode reports `buy_single`/`sell_single`, which carry no
 * leg — reported as `both` rather than guessed, so a one-way account is never described as
 * hedged.
 */
const positionSideOf = (row: Record<string, any>): PositionSide | 'both' => {
  const tradeSide = String(row?.tradeSide ?? '').toLowerCase();
  if (tradeSide.endsWith('_single') || !tradeSide) return 'both';
  if (tradeSide.includes('long')) return PositionSide.Long;
  if (tradeSide.includes('short')) return PositionSide.Short;
  return 'both';
};

export type BitgetFillDeps = {
  /** Venue symbol -> normalized symbol. `undefined` drops the row rather than guessing. */
  resolveSymbol: (venueSymbol: string) => string | undefined;
  /** Convert a venue ms timestamp into whatever the caller's Execution uses. */
  toTimestamp: (ms: number) => number;
};

export const mapBitgetFill = (row: Record<string, any>, deps: BitgetFillDeps): Execution | null => {
  const venueSymbol = String(row?.symbol ?? '');
  const symbol = deps.resolveSymbol(venueSymbol);
  if (!symbol) return null;

  const ms = parseInt(String(row?.cTime ?? '0'), 10);
  if (!Number.isFinite(ms) || ms <= 0) return null;

  const qty = num(row?.baseVolume);
  const price = num(row?.price);
  if (!(qty > 0) || !(price > 0)) return null;

  const fee = totalFeeOf(row);

  return {
    timestamp: deps.toTimestamp(ms),
    symbol,
    id: row?.tradeId ? String(row.tradeId) : undefined,
    orderId: row?.orderId ? String(row.orderId) : undefined,
    price,
    qty,
    side: String(row?.side ?? '').toLowerCase() === 'buy' ? OrderSide.Buy : OrderSide.Sell,
    // Canonical `commission` is positive-when-charged; bitget signs `totalFee` negative, so
    // this is a negation, NOT an absolute value — a rebate must survive as a rebate.
    commission: -fee,
    commissionAsset: String(row?.feeDetail?.[0]?.feeCoin ?? 'USDT'),
    realizedPnl: num(row?.profit),
    positionSide: positionSideOf(row),
    maker: String(row?.tradeScope ?? '').toLowerCase() === 'maker',
  } as Execution;
};

export const mapBitgetFills = (rows: Array<Record<string, any>>, deps: BitgetFillDeps): Execution[] => {
  const executions: Execution[] = [];
  for (const row of rows) {
    const execution = mapBitgetFill(row, deps);
    if (execution) executions.push(execution);
  }
  return executions;
};
