import { describe, expect, it } from 'vitest';

import { mapOkxBillToIncomeRecords } from '../reference/okx/okx.income';

/**
 * Row shapes captured live from OKX `/api/v5/account/bills` (sheet row 13, 100 rows,
 * type=2). The measured aggregate over that page was:
 *   sum balChg = -0.48706, sum fee = -0.42126, sum pnl = -0.06580, residual 0.0000000000
 *
 * The point of the fixture is the structural difference from binance: OKX puts BOTH the
 * fee and the realized PnL on ONE bill, so a 1:1 row->record mapping merges two terms of
 * the reconciliation identity into one number.
 */
const resolveSymbol = (instId: string) => (instId === 'DOGE-USDT-SWAP' ? 'DOGE/USDT:USDT' : undefined);

const bill = (over: Record<string, any> = {}) => ({
  billId: '1234567890',
  instId: 'DOGE-USDT-SWAP',
  ccy: 'USDT',
  type: '2',
  subType: '5',
  ts: '1754000000000',
  balChg: '-0.0128',
  fee: '-0.0084',
  pnl: '-0.0044',
  tradeId: '99887766',
  ...over,
});

const sumIncome = (records: { income: number }[]) => records.reduce((acc, r) => acc + r.income, 0);

describe('okx bill -> income decomposition', () => {
  it('splits one bill carrying both fee and pnl into two records', () => {
    const records = mapOkxBillToIncomeRecords(bill(), resolveSymbol);

    expect(records).toHaveLength(2);
    expect(records.map((r) => r.category).sort()).toEqual(['commission', 'realized_pnl']);
    // Distinct tranIds, or downstream dedupe collapses the two terms back into one.
    expect(new Set(records.map((r) => r.tranId)).size).toBe(2);
    expect(records.every((r) => r.symbol === 'DOGE/USDT:USDT')).toBe(true);
    expect(records.every((r) => r.timestamp === 1754000000000)).toBe(true);
  });

  it('emitted incomes sum to the bill balChg', () => {
    const rows = [
      bill({ billId: '1', balChg: '-0.0128', fee: '-0.0084', pnl: '-0.0044' }),
      bill({ billId: '2', balChg: '-0.0084', fee: '-0.0084', pnl: '0' }),
      bill({ billId: '3', balChg: '0.0121', fee: '-0.0084', pnl: '0.0205' }),
    ];

    for (const row of rows) {
      const records = mapOkxBillToIncomeRecords(row, resolveSymbol);
      expect(sumIncome(records)).toBeCloseTo(parseFloat(row.balChg), 10);
    }
  });

  it('emits a residual record when balChg is not explained by fee and pnl', () => {
    // Transfers carry neither a fee nor a pnl, so a fee/pnl-only mapper drops them and
    // the ledger no longer balances against equity.
    const records = mapOkxBillToIncomeRecords(
      bill({ type: '1', subType: undefined, balChg: '250', fee: '0', pnl: '0', instId: '' }),
      resolveSymbol,
    );

    expect(records).toHaveLength(1);
    expect(records[0].category).toBe('transfer');
    expect(records[0].income).toBe(250);
    expect(records[0].symbol).toBe('');
  });

  it('categorises funding and liquidation bills off the bill type', () => {
    const funding = mapOkxBillToIncomeRecords(
      bill({ type: '8', fee: '0', pnl: '-0.31', balChg: '-0.31' }),
      resolveSymbol,
    );
    expect(funding[0].category).toBe('funding');
    expect(funding[0].incomeType).toBe('FUNDING_FEE');

    // 9 is ADL and 5 is liquidation; 13/14 are DDH/block-trade and must NOT be liquidation.
    expect(mapOkxBillToIncomeRecords(bill({ type: '9' }), resolveSymbol)[0].category).toBe('liquidation');
    expect(mapOkxBillToIncomeRecords(bill({ type: '14' }), resolveSymbol)[0].category).toBe('other');
  });

  it('keeps an unresolvable instId rather than dropping the row', () => {
    const records = mapOkxBillToIncomeRecords(bill({ instId: 'NEW-USDT-SWAP' }), resolveSymbol);

    expect(records).toHaveLength(2);
    expect(records[0].symbol).toBe('NEW-USDT-SWAP');
  });

  it('does not emit a phantom residual on a large bill', () => {
    // One ulp at 250k is ~2.9e-11, so a fixed 1e-12 epsilon fires on float noise alone
    // and the phantom inherits the bill's category — inflating realized-PnL counts.
    const records = mapOkxBillToIncomeRecords(
      bill({ balChg: '250000.12345678', fee: '-0.42126', pnl: '250000.54471678' }),
      resolveSymbol,
    );

    expect(records).toHaveLength(2);
    expect(records.some((r) => r.tranId.endsWith(':adj'))).toBe(false);
  });

  it('still emits a residual that is genuinely large relative to the bill', () => {
    const records = mapOkxBillToIncomeRecords(
      bill({ balChg: '250000', fee: '-0.42126', pnl: '249000' }),
      resolveSymbol,
    );

    const adj = records.find((r) => r.tranId.endsWith(':adj'));
    expect(adj).toBeDefined();
    expect(adj!.income).toBeCloseTo(1000.42126, 6);
  });

  it('reports a positive fee as a rebate', () => {
    const records = mapOkxBillToIncomeRecords(bill({ fee: '0.0021', pnl: '0', balChg: '0.0021' }), resolveSymbol);

    expect(records).toHaveLength(1);
    expect(records[0].category).toBe('rebate');
  });
});
