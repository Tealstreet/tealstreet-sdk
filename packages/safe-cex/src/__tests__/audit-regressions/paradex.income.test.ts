import { describe, expect, it } from 'vitest';

import { mapParadexFillToIncomeRecords } from '../reference/paradex/paradex.income';

const resolveSymbol = (market: string) => (market === 'DOGE-USD-PERP' ? 'DOGE-USD-PERP' : undefined);

/** Shape captured live from paradex `/fills` (sheet row 81), not inferred. */
const fill = (over: Record<string, any> = {}) => ({
  id: '1786522543900201709221520001',
  side: 'BUY',
  liquidity: 'TAKER',
  market: 'DOGE-USD-PERP',
  order_id: '1786522543860201709229320000',
  price: '0.07168',
  size: '170',
  fee: '0.0054846218605318',
  fee_currency: 'USDC',
  created_at: 1786522543911,
  fill_type: 'FILL',
  realized_pnl: '-0.030606148775289',
  realized_funding: '0',
  ...over,
});

describe('paradex fill -> income', () => {
  it('emits realized pnl and commission from one row', () => {
    const records = mapParadexFillToIncomeRecords(fill(), resolveSymbol);

    expect(records.map((r) => r.category).sort()).toEqual(['commission', 'realized_pnl']);
    expect(new Set(records.map((r) => r.tranId)).size).toBe(2);
  });

  it('emits all three terms when funding is non-zero', () => {
    // Paradex is the only venue here that carries funding on the fill itself, so its whole
    // ledger comes from one endpoint.
    const records = mapParadexFillToIncomeRecords(fill({ realized_funding: '-0.004' }), resolveSymbol);

    expect(records.map((r) => r.category).sort()).toEqual(['commission', 'funding', 'realized_pnl']);
    expect(records.find((r) => r.category === 'funding')!.income).toBeCloseTo(-0.004, 12);
  });

  it('reports a charged fee as negative income, not an absolute value', () => {
    // fetchHistoricalFills in this directory does Math.abs(fee); copying that erases rebates.
    const commission = mapParadexFillToIncomeRecords(fill(), resolveSymbol).find((r) => r.category === 'commission');

    expect(commission!.income).toBeCloseTo(-0.0054846218605318, 12);
  });

  it('treats a negative fee as a rebate', () => {
    const records = mapParadexFillToIncomeRecords(
      fill({ fee: '-0.002', realized_pnl: '0', realized_funding: '0' }),
      resolveSymbol,
    );

    expect(records).toHaveLength(1);
    expect(records[0].category).toBe('rebate');
    expect(records[0].income).toBeCloseTo(0.002, 12);
  });

  it('keeps created_at in MILLISECONDS', () => {
    // fetchHistoricalFills normalises to seconds; IncomeRecord.timestamp is ms on every venue,
    // and a seconds value here would place every row in 1970.
    expect(mapParadexFillToIncomeRecords(fill(), resolveSymbol)[0].timestamp).toBe(1786522543911);
  });

  it('keys tranId on the fill id so identical partials do not collapse', () => {
    const a = mapParadexFillToIncomeRecords(fill({ id: 'fill-a' }), resolveSymbol);
    const b = mapParadexFillToIncomeRecords(fill({ id: 'fill-b' }), resolveSymbol);

    expect(a[0].tranId).not.toBe(b[0].tranId);
  });

  it('sums to the row total, so the ledger conserves value', () => {
    const row = fill({ realized_pnl: '1.5', fee: '0.02', realized_funding: '-0.3' });
    const total = mapParadexFillToIncomeRecords(row, resolveSymbol).reduce((acc, r) => acc + r.income, 0);

    expect(total).toBeCloseTo(1.5 - 0.02 - 0.3, 12);
  });

  it('falls back to the raw market rather than dropping an unresolvable one', () => {
    expect(mapParadexFillToIncomeRecords(fill({ market: 'NEW-USD-PERP' }), resolveSymbol)[0].symbol).toBe(
      'NEW-USD-PERP',
    );
  });
});
