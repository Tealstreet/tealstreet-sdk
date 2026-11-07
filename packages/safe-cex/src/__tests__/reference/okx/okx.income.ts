import type { IncomeCategory, IncomeRecord } from '../../../types';

/**
 * OKX bill -> IncomeRecord decomposition.
 *
 * Pure and exported on purpose: mapping logic that lives as an instance field on the
 * exchange class cannot be unit tested, which is how the WOO hedge-side bug survived.
 *
 * The shape differs from binance in a way that matters. Binance emits ONE row per
 * category, so a row maps 1:1 to an IncomeRecord. OKX emits one bill row carrying BOTH
 * `fee` and `pnl`, measured live over 100 rows:
 *   sum balChg = -0.48706, sum fee = -0.42126, sum pnl = -0.06580, residual 0.0000000000
 * Mapping such a row to a single record silently merges two terms of the reconciliation
 * identity (realized PnL and commission), so one bill becomes up to three records.
 *
 * The invariant every caller may rely on: the emitted incomes for a bill sum to that
 * bill's `balChg`. The `:adj` residual term exists to keep that true for bill types
 * whose balance change is not explained by fee + pnl (transfers, interest, conversions).
 */

// Bill `type` codes, per the ccxt listing vendored at v2/ccxt/python/ccxt/okx.py.
// Deliberately not exhaustive — anything unmapped falls through to 'other', which is
// still counted, just not attributed.
const BILL_TYPE_CATEGORY: Record<string, IncomeCategory> = {
  '1': 'transfer',
  '2': 'realized_pnl',
  '5': 'liquidation',
  '6': 'transfer',
  '8': 'funding',
  '9': 'liquidation', // ADL
  '12': 'transfer', // strategy transfer
};

// Canonical uppercase strings matching what binance reports and what bybit/weex already
// rewrite their raw types into. `IncomeRecord.incomeType` is documented as the venue's
// raw string, but OKX's raw type is a bare number ("2") that no consumer can interpret,
// and `apps/web/src/components/journal` switches on the binance spellings.
const CATEGORY_INCOME_TYPE: Record<IncomeCategory, string> = {
  realized_pnl: 'REALIZED_PNL',
  funding: 'FUNDING_FEE',
  commission: 'COMMISSION',
  rebate: 'REBATE',
  transfer: 'TRANSFER',
  liquidation: 'LIQUIDATION',
  other: 'OTHER',
};

/**
 * Canonical income type -> OKX bill `type` filter. COMMISSION maps to trade bills
 * because OKX carries the fee on the trade row itself rather than on its own row.
 */
export const OKX_BILL_TYPE_BY_INCOME_TYPE: Record<string, string | undefined> = {
  REALIZED_PNL: '2',
  COMMISSION: '2',
  REBATE: '2',
  FUNDING_FEE: '8',
};

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const okxBillCategory = (type: unknown): IncomeCategory => {
  return BILL_TYPE_CATEGORY[String(type ?? '')] || 'other';
};

export const okxIncomeTypeForCategory = (category: IncomeCategory) => CATEGORY_INCOME_TYPE[category];

export const mapOkxBillToIncomeRecords = (
  row: Record<string, any>,
  resolveSymbol: (instId: string) => string | undefined,
): IncomeRecord[] => {
  const timestamp = parseInt(String(row.ts ?? '0'), 10);
  if (!Number.isFinite(timestamp) || timestamp <= 0) {
    return [];
  }

  const balChg = toNumber(row.balChg);
  const fee = toNumber(row.fee);
  const pnl = toNumber(row.pnl);

  const instId = String(row.instId || '');
  // Falling back to the raw instId rather than dropping the row: an unresolvable market
  // must not silently remove value from the ledger and break the sum-to-balChg invariant.
  const symbol = instId ? resolveSymbol(instId) || instId : '';
  const asset = String(row.ccy || '');
  const billId = String(row.billId || row.ts || '');
  const tradeId = row.tradeId ? String(row.tradeId) : undefined;
  const category = okxBillCategory(row.type);

  const records: IncomeRecord[] = [];

  const push = (income: number, recordCategory: IncomeCategory, suffix: string) => {
    records.push({
      symbol,
      incomeType: okxIncomeTypeForCategory(recordCategory),
      income,
      asset,
      timestamp,
      tranId: `${billId}:${suffix}`,
      ...(tradeId ? { tradeId } : {}),
      category: recordCategory,
    });
  };

  if (pnl !== 0) {
    push(pnl, category, 'pnl');
  }

  if (fee !== 0) {
    push(fee, fee < 0 ? 'commission' : 'rebate', 'fee');
  }

  // The threshold must be RELATIVE. Float noise scales with magnitude — one ulp at
  // balChg=250000 is ~2.9e-11, so a fixed 1e-12 emits a phantom record for every
  // large bill, and a phantom carrying the bill's category inflates realized-PnL
  // trade counts downstream.
  const residual = balChg - fee - pnl;
  const residualEpsilon = Math.max(1e-12, 1e-9 * Math.abs(balChg));
  if (Math.abs(residual) > residualEpsilon) {
    push(residual, category, 'adj');
  }

  return records;
};
