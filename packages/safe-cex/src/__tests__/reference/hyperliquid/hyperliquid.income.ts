import type { ClosedPnlEntry, IncomeRecord } from '../../../types';

import { PositionSide } from '../../../types';

/**
 * Hyperliquid ledger mapping — fills and funding to `IncomeRecord`.
 *
 * Pure and exported so it is testable; the exchange methods are instance fields.
 *
 * Hyperliquid has no separate fee row: the fee lives ON the fill, alongside that fill's
 * realized PnL. So one fill emits up to two records, and `closedPnl` EXCLUDES the fee —
 * confirmed by `handleFills` in `hyperliquid.ws-private.ts` computing
 * `netRealizedPnl = closedPnl - fee`. Mapping `closedPnl` alone would drop every fee from
 * the ledger, the same defect bybit had.
 *
 * Two traps that a careless mapper hits and that no typecheck catches:
 *
 * - **`fee` can be NEGATIVE** (maker rebate). `fetchHistoricalFills` in this directory does
 *   `Math.abs(feeRaw)`, which is already wrong for rebates — do not copy it. The emitted
 *   income is `-fee`, and its sign decides `commission` vs `rebate`.
 * - **REST and WS funding rows have DIFFERENT SHAPES.** REST `UserFundingEntry` nests
 *   everything under `delta` (`{ time, hash, delta: { coin, usdc, ... } }`) while the WS
 *   `WsUserFunding` is flat (`{ time, coin, usdc, ... }`). One mapper reading the flat
 *   shape against REST rows silently yields `undefined` coin and usdc — it does not throw.
 *   `readFundingRow` normalises both.
 *
 * `tranId` uses the fill's `tid` and funding's `hash`+`coin`, never a
 * price/size/time composite: `fetchHistoricalFills` dedupes on
 * `orderId:ts:price:qty:side`, which collapses genuine identical partial fills.
 */

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

/** Accepts the nested REST row and the flat WS row. */
const readFundingRow = (row: Record<string, any>) => {
  const delta = (row.delta ?? row) as Record<string, any>;
  return {
    coin: String(delta.coin ?? row.coin ?? ''),
    usdc: toNumber(delta.usdc ?? row.usdc),
    hash: String(row.hash ?? ''),
    time: Number(row.time ?? 0),
  };
};

export const mapHyperliquidFillToIncomeRecords = (
  fill: Record<string, any>,
  resolveSymbol: (coin: string) => string | undefined,
): IncomeRecord[] => {
  const time = Number(fill.time ?? 0);
  const coin = String(fill.coin ?? '');
  if (!time || !coin) {
    return [];
  }

  const closedPnl = toNumber(fill.closedPnl);
  const fee = toNumber(fill.fee);
  // Falling back to the raw coin rather than dropping: an unresolvable market must not
  // remove value from the ledger.
  const symbol = resolveSymbol(coin) || coin;
  const asset = String(fill.feeToken || 'USDC');
  const tid = String(fill.tid ?? fill.hash ?? `${time}`);

  const records: IncomeRecord[] = [];

  if (closedPnl !== 0) {
    records.push({
      symbol,
      incomeType: 'REALIZED_PNL',
      income: closedPnl,
      asset,
      timestamp: time,
      tranId: `${tid}:pnl`,
      tradeId: tid,
      category: 'realized_pnl',
    });
  }

  if (fee !== 0) {
    records.push({
      symbol,
      incomeType: fee > 0 ? 'COMMISSION' : 'REBATE',
      income: -fee,
      asset,
      timestamp: time,
      tranId: `${tid}:fee`,
      tradeId: tid,
      category: fee > 0 ? 'commission' : 'rebate',
    });
  }

  return records;
};

export const mapHyperliquidFundingToIncomeRecord = (
  row: Record<string, any>,
  resolveSymbol: (coin: string) => string | undefined,
): IncomeRecord | null => {
  const { coin, usdc, hash, time } = readFundingRow(row);
  if (!time || !coin || usdc === 0) {
    return null;
  }

  return {
    symbol: resolveSymbol(coin) || coin,
    incomeType: 'FUNDING_FEE',
    income: usdc,
    asset: 'USDC',
    timestamp: time,
    tranId: `${hash || time}:${coin}:funding`,
    category: 'funding',
  };
};

/**
 * Walks a time-ranged `/info` history type forward until it stops returning new rows.
 *
 * Hyperliquid bounds a response by ROW COUNT, not window width, so paging on time windows
 * would either miss rows or issue needless requests.
 *
 * The cursor is INCLUSIVE and duplicates are removed by the venue's own id. The tempting
 * alternative — advancing to `newest + 1` — silently drops rows sharing the boundary
 * millisecond, which is not exotic: one order sweeping several levels emits
 * same-millisecond fills and the row cap can land in the middle of them. Identity dedupe
 * is safe HERE specifically because `tid` is genuinely unique (unlike bybit's tranId,
 * where the same approach deleted real rows).
 *
 * Exported rather than left as an instance field so the boundary behaviour is testable —
 * a live 30-day window fits in one page, so paging is never exercised in practice.
 */
export const paginateHyperliquidByTime = async (
  post: (type: string, body: Record<string, any>) => Promise<any[]>,
  type: string,
  startTime: number,
  endTime: number,
  keyOf: (row: Record<string, any>) => string,
  onCapHit?: (type: string, pages: number) => void,
  maxPages = 200,
) => {
  const collected: Array<Record<string, any>> = [];
  const seen = new Set<string>();

  let cursor = startTime;
  let page = 0;

  for (; page < maxPages; page += 1) {
    const rows = await post(type, { startTime: cursor, endTime });
    if (rows.length === 0) break;

    let newest = cursor;
    let added = 0;
    for (const row of rows) {
      const key = keyOf(row);
      if (seen.has(key)) continue;
      seen.add(key);
      collected.push(row);
      added += 1;
      newest = Math.max(newest, Number(row.time ?? 0));
    }

    // Nothing new means the page was entirely overlap — the window is exhausted.
    if (added === 0) break;
    cursor = newest;
  }

  if (page >= maxPages) onCapHit?.(type, maxPages);

  return collected;
};

/**
 * Folds close-fills into `ClosedPnlEntry[]`.
 *
 * Hyperliquid has no closed-position endpoint, so this is the only way to get the axis —
 * but it is a RESHAPING of the fills, not a second observation of them. Every entry is
 * therefore stamped `provenance: 'derived'`, which is what stops the cross-axis oracle
 * from "verifying" the ledger against data folded from the same rows and reporting a
 * meaningless agreement.
 *
 * Two honest limitations, both consequences of the source rather than of this code:
 *
 * - **`entryPrice` is not available.** A fill knows its own execution price and the
 *   position size before it (`startPosition`), not the average entry of the position it
 *   is closing. It is left `0` rather than approximated, because a plausible-looking
 *   wrong entry price silently corrupts every return-percentage downstream.
 * - **Granularity differs from a venue-reported axis.** Bybit returns one row per closed
 *   POSITION; this returns one per closing FILL, so a position closed in three fills
 *   yields three entries. Summing PnL is unaffected; counting "trades" is not.
 */
export const deriveHyperliquidClosedPositions = (
  fills: Array<Record<string, any>>,
  resolveSymbol: (coin: string) => string | undefined,
  accountName: string,
): ClosedPnlEntry[] => {
  const entries: ClosedPnlEntry[] = [];

  for (const fill of fills) {
    const dir = String(fill.dir ?? '');
    if (!dir.toLowerCase().startsWith('close')) continue;

    const time = Number(fill.time ?? 0);
    const coin = String(fill.coin ?? '');
    if (!time || !coin) continue;

    const px = toNumber(fill.px);
    const sz = toNumber(fill.sz);

    entries.push({
      provenance: 'derived',
      symbol: resolveSymbol(coin) || coin,
      notional: px * sz,
      contracts: sz,
      // Not knowable from a fill — see the note above. Deliberately 0, not a guess.
      entryPrice: 0,
      exitPrice: px,
      // Legacy field name; carries realized PnL on a closed row.
      unrealizedPnl: toNumber(fill.closedPnl),
      side: dir.toLowerCase().includes('long') ? PositionSide.Long : PositionSide.Short,
      fees: toNumber(fill.fee),
      time,
      closedTime: time,
      account: accountName,
      leverage: 0,
    });
  }

  return entries;
};
