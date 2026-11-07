import { describe, expect, it } from 'vitest';

import { buildWooIncomeParams, mapWooIncomeRecord, wooIncomeIdentity } from '../reference/woo/woo.income';

const aggregate = { id: 1786603865910, type: 'REALIZED_PNL', token: 'USDT', amount: 3.5, timestamp: 1786603865910 };

describe('WOO V1 income ledger', () => {
  it('keeps account PnL without inventing a symbol or per-position identity', () => {
    expect(mapWooIncomeRecord(aggregate)).toEqual({
      symbol: '',
      incomeType: 'REALIZED_PNL',
      asset: 'USDT',
      income: 3.5,
      timestamp: 1786603865910,
      tranId: '1786603865910',
      category: 'realized_pnl',
    });
  });

  it('preserves charged fee signs and genuine zero cash changes', () => {
    expect(
      mapWooIncomeRecord({ ...aggregate, type: 'TRANSACTION_FEE', symbol: 'PERP_BTC_USDT', amount: -0.1 }),
    ).toMatchObject({ symbol: 'BTCUSDT', income: -0.1, category: 'commission' });
    expect(mapWooIncomeRecord({ ...aggregate, amount: 0 })?.income).toBe(0);
  });

  it('does not interpret borrow interest as funding', () => {
    expect(mapWooIncomeRecord({ ...aggregate, type: 'INTEREST', amount: -0.01 })?.category).toBe('other');
    expect(mapWooIncomeRecord({ ...aggregate, type: 'FUNDING_FEE' })?.category).toBe('funding');
  });

  it('preserves exact string IDs and distinguishes records sharing an ID', () => {
    const pnl = mapWooIncomeRecord({ ...aggregate, id: '9007199254740993' })!;
    const fee = mapWooIncomeRecord({ ...aggregate, id: '9007199254740993', type: 'TRANSACTION_FEE' })!;
    expect(pnl.tranId).toBe('9007199254740993');
    expect(wooIncomeIdentity(pnl)).not.toBe(wooIncomeIdentity(fee));
  });

  it('rejects missing money, malformed rows and already-rounded numeric IDs', () => {
    for (const row of [
      null,
      {},
      { ...aggregate, amount: null },
      { ...aggregate, amount: '' },
      { ...aggregate, timestamp: 'NaN' },
      { ...aggregate, id: 9007199254740992 },
      { ...aggregate, symbol: 1 },
    ]) {
      expect(mapWooIncomeRecord(row)).toBeNull();
    }
  });

  it('uses the ledger millisecond window and working page vocabulary', () => {
    expect(
      buildWooIncomeParams(
        { symbol: 'BTCUSDT', incomeType: 'REALIZED_PNL', startTime: 1786600000000, endTime: 1786700000000, limit: 500 },
        2,
      ),
    ).toEqual({
      type: 'REALIZED_PNL',
      start_t: 1786600000000,
      end_t: 1786700000000,
      size: 100,
      page: 2,
    });
  });
});
