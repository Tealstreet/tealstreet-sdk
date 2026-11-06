import type { IncomeRecord } from '../../../types';

/**
 * Paradex ledger mapping — one `/fills` row carries every term.
 *
 * Field names verified against a live account (row 81) rather than inferred:
 * `id, side, liquidity, market, order_id, price, size, fee, fee_currency, created_at,
 *  remaining_size, client_id, fill_type, realized_pnl, realized_funding, account,
 *  underlying_price, flags, orderbook_seq_no`.
 *
 * So paradex is the cheapest ledger of any venue here: realized PnL, commission AND
 * funding all ride on the fill, and no separate funding request is needed (hyperliquid
 * needs one, and its `/funding/payments` answered 401 under our JWT scope anyway).
 *
 * `created_at` is epoch MILLISECONDS — do not reuse `fetchHistoricalFills`'s
 * `normalizeToSeconds` in this file; `IncomeRecord.timestamp` is ms on every venue.
 *
 * `fee` is positive when charged, so the emitted income is `-fee` and its sign decides
 * `commission` vs `rebate`. Do not copy `fetchHistoricalFills`'s `Math.abs(fee)` — that erases
 * maker rebates, the same defect found on hyperliquid.
 */

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const mapParadexFillToIncomeRecords = (
  row: Record<string, any>,
  resolveSymbol: (market: string) => string | undefined,
): IncomeRecord[] => {
  const timestamp = Number(row.created_at ?? 0);
  const market = String(row.market ?? '');
  if (!timestamp || !market) {
    return [];
  }

  // Falling back to the raw market id rather than dropping: an unresolvable market must
  // not silently remove value from the ledger.
  const symbol = resolveSymbol(market) || market;
  const asset = String(row.fee_currency || 'USDC');
  // Paradex gives every fill a unique `id`; keying on price/size/time would collapse
  // genuine identical partial fills.
  const id = String(row.id ?? `${timestamp}:${market}:${row.order_id ?? ''}`);

  const realizedPnl = toNumber(row.realized_pnl);
  const fee = toNumber(row.fee);
  const funding = toNumber(row.realized_funding);

  const records: IncomeRecord[] = [];
  const push = (income: number, category: IncomeRecord['category'], incomeType: string, suffix: string) => {
    records.push({
      symbol,
      incomeType,
      income,
      asset,
      timestamp,
      tranId: `${id}:${suffix}`,
      tradeId: id,
      category,
    });
  };

  if (realizedPnl !== 0) {
    push(realizedPnl, 'realized_pnl', 'REALIZED_PNL', 'pnl');
  }

  if (fee !== 0) {
    push(-fee, fee > 0 ? 'commission' : 'rebate', fee > 0 ? 'COMMISSION' : 'REBATE', 'fee');
  }

  if (funding !== 0) {
    push(funding, 'funding', 'FUNDING_FEE', 'funding');
  }

  return records;
};
