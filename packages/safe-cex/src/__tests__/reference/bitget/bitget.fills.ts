import type { Execution } from '../../../types';

import { OrderSide, PositionSide } from '../../../types';

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

const optionalNum = (value: unknown): number | undefined => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : undefined;
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

const totalUtaFeeOf = (row: Record<string, any>): number | undefined => {
  const detail = row?.feeDetail;
  if (!Array.isArray(detail)) return optionalNum(row?.fee);

  let total = 0;
  let found = false;
  for (const entry of detail) {
    const fee = optionalNum(entry?.fee ?? entry?.totalFee);
    if (fee === undefined) continue;
    total += fee;
    found = true;
  }

  return found ? total : undefined;
};

/**
 * Hedge leg from `tradeSide`. One-way mode reports `buy_single`/`sell_single`, which carry no
 * leg — reported as `both` rather than guessed, so a one-way account is never described as
 * hedged.
 */
/**
 * Leg AND direction, which on this venue are NOT the same question — and conflating them is
 * what made bitget SKIP.
 *
 * **MEASURED 2026-08-14 on sheet row 96.** In hedge mode this account reports `tradeSide` as a
 * bare `open`/`close` with no leg in it, and reports `side` as the **POSITION side, not the
 * order direction**. Both fills of one short round trip say `side: "sell"`:
 *
 * | cTime         | side | tradeSide | price   | profit  | ledger `businessType` |
 * | ------------- | ---- | --------- | ------- | ------- | --------------------- |
 * | 1786693269342 | sell | open      | 0.33424 | 0       | `open_short`          |
 * | 1786693310040 | sell | close     | 0.33425 | -0.0003 | `close_short`         |
 *
 * `(0.33424 - 0.33425) x 30 = -0.0003` exactly, so it is unambiguously a short: opened low,
 * closed higher, small loss. Yet a naive read of `side` makes the CLOSE another sell — the
 * fold deepens the short instead of flattening it, the interval never closes, and bitget
 * reports intervals that never close while the venue says the account is FLAT.
 *
 * So the fold's direction must be derived from `(leg, open|close)`, never copied from `side`.
 *
 * The old version returned `'both'` for a bare `open`/`close`, which merged hedge longs and
 * shorts into one bucket where they netted against each other. **A default that silently
 * merges legs is the same shape as a fabricated zero** — it must be explicit or refuse.
 */
type BitgetLeg = { positionSide: PositionSide | 'both'; direction: OrderSide };

const legAndDirectionOf = (row: Record<string, any>): BitgetLeg | null => {
  const tradeSide = String(row?.tradeSide ?? '').toLowerCase();
  const rawSide = String(row?.side ?? '').toLowerCase();
  if (rawSide !== 'buy' && rawSide !== 'sell') return null;

  // ONE-WAY: `buy_single` / `sell_single`. There is no leg, and `side` IS the order
  // direction — the two modes disagree about what `side` means, which is why they are
  // separated here rather than sharing a path.
  if (tradeSide.endsWith('_single')) {
    return { positionSide: 'both', direction: rawSide === 'buy' ? OrderSide.Buy : OrderSide.Sell };
  }

  // HEDGE, explicit spelling (`open_long` / `close_short` / ...). The leg is stated.
  const explicitLeg = tradeSide.includes('long')
    ? PositionSide.Long
    : tradeSide.includes('short')
      ? PositionSide.Short
      : undefined;

  // HEDGE, bare spelling (`open` / `close`). The leg is `side`, per the table above.
  const leg = explicitLeg ?? (rawSide === 'buy' ? PositionSide.Long : PositionSide.Short);
  const opening = tradeSide.startsWith('open');
  const closing = tradeSide.startsWith('close');

  // Anything else is an unrecognised vocabulary. REFUSE rather than fall through to `'both'`:
  // guessing here merges legs silently, and the fold cannot tell a merged leg from a real one.
  if (!opening && !closing) return null;

  // Direction that moves the leg the right way: a long opens by buying and closes by
  // selling; a short is the mirror.
  const direction =
    leg === PositionSide.Long
      ? opening
        ? OrderSide.Buy
        : OrderSide.Sell
      : opening
        ? OrderSide.Sell
        : OrderSide.Buy;

  return { positionSide: leg, direction };
};

const utaLegAndDirectionOf = (row: Record<string, any>): BitgetLeg | null => {
  const tradeSide = String(row?.tradeSide ?? '').toLowerCase();
  const rawSide = String(row?.side ?? '').toLowerCase();
  if (rawSide !== 'buy' && rawSide !== 'sell') return null;

  const direction = rawSide === 'buy' ? OrderSide.Buy : OrderSide.Sell;
  const explicitLeg =
    String(row?.posSide ?? row?.holdSide ?? '').toLowerCase() === 'long'
      ? PositionSide.Long
      : String(row?.posSide ?? row?.holdSide ?? '').toLowerCase() === 'short'
        ? PositionSide.Short
        : tradeSide.includes('long')
          ? PositionSide.Long
          : tradeSide.includes('short')
            ? PositionSide.Short
            : undefined;

  if (explicitLeg) {
    return { positionSide: explicitLeg, direction };
  }

  const opening = tradeSide.startsWith('open');
  const closing = tradeSide.startsWith('close');
  if (!opening && !closing) return null;

  const positionSide =
    direction === OrderSide.Buy
      ? opening
        ? PositionSide.Long
        : PositionSide.Short
      : opening
        ? PositionSide.Short
        : PositionSide.Long;

  return { positionSide, direction };
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

  // Dropped rather than defaulted: an unrecognised `tradeSide`/`side` combination cannot be
  // placed on a leg, and placing it on the wrong one corrupts every later interval.
  const leg = legAndDirectionOf(row);
  if (!leg) return null;

  return {
    timestamp: deps.toTimestamp(ms),
    symbol,
    id: row?.tradeId ? String(row.tradeId) : undefined,
    orderId: row?.orderId ? String(row.orderId) : undefined,
    price,
    qty,
    // DERIVED, never copied from `row.side` — see `legAndDirectionOf`. On hedge fills this
    // venue puts the POSITION side in `side`, so copying it makes every close look like
    // another open.
    side: leg.direction,
    // Canonical `commission` is positive-when-charged; bitget signs `totalFee` negative, so
    // this is a negation, NOT an absolute value — a rebate must survive as a rebate.
    commission: -fee,
    commissionAsset: String(row?.feeDetail?.[0]?.feeCoin ?? 'USDT'),
    realizedPnl: num(row?.profit),
    positionSide: leg.positionSide,
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

export const mapBitgetUtaFill = (row: Record<string, any>, deps: BitgetFillDeps): Execution | null => {
  const venueSymbol = String(row?.symbol ?? '');
  const symbol = deps.resolveSymbol(venueSymbol);
  if (!symbol) return null;

  const ms = parseInt(String(row?.createdTime ?? '0'), 10);
  if (!Number.isFinite(ms) || ms <= 0) return null;

  const qty = num(row?.execQty);
  const price = num(row?.execPrice);
  if (!(qty > 0) || !(price > 0)) return null;

  const leg = utaLegAndDirectionOf(row);
  if (!leg) return null;

  const fee = totalUtaFeeOf(row);
  const commissionAsset = row?.feeDetail?.[0]?.feeCoin ? String(row.feeDetail[0].feeCoin) : undefined;

  return {
    timestamp: deps.toTimestamp(ms),
    symbol,
    id: row?.execId ? String(row.execId) : row?.execLinkId ? String(row.execLinkId) : undefined,
    orderId: row?.orderId ? String(row.orderId) : undefined,
    price,
    qty,
    side: leg.direction,
    commission: fee,
    commissionAsset,
    realizedPnl: optionalNum(row?.execPnl),
    positionSide: leg.positionSide,
    maker: row?.tradeScope ? String(row.tradeScope).toLowerCase() === 'maker' : undefined,
  } as Execution;
};

export const mapBitgetUtaFills = (rows: Array<Record<string, any>>, deps: BitgetFillDeps): Execution[] => {
  const executions: Execution[] = [];
  for (const row of rows) {
    const execution = mapBitgetUtaFill(row, deps);
    if (execution) executions.push(execution);
  }
  return executions;
};
