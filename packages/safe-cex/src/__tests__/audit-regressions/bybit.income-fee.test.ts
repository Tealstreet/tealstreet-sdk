import type { BybitTransactionRecord } from '../../exchanges/bybit/bybit.types';

import { describe, expect, it } from 'vitest';

import { mapBybitTransactionToIncomeRecords } from '../reference/bybit/bybit.income';

/**
 * Bybit's income mapper read ONLY `cashFlow`. `record.fee` was never referenced, so every
 * trading fee vanished from the ledger — the commission term of the reconciliation
 * identity was absent rather than wrong, which is undetectable downstream.
 *
 * Bybit reports cashFlow, funding and fee as three independent terms on one row, so a row
 * maps to several records. `fee` is positive when charged, hence the emitted income is -fee.
 */
const row = (over: Partial<BybitTransactionRecord> = {}): BybitTransactionRecord =>
  ({
    symbol: 'DOGEUSDT',
    category: 'linear',
    side: 'Buy',
    transactionTime: '1754000000000',
    type: 'TRADE',
    qty: '100',
    size: '100',
    currency: 'USDT',
    tradePrice: '0.1',
    funding: '0',
    fee: '0.0055',
    cashFlow: '-0.42',
    change: '-0.4255',
    cashBalance: '100',
    feeRate: '0.00055',
    bonusChange: '0',
    tradeId: 'trade-1',
    orderId: 'order-1',
    orderLinkId: '',
    ...over,
  }) as BybitTransactionRecord;

describe('bybit transaction -> income decomposition', () => {
  it('emits the trading fee that the previous mapper dropped', () => {
    const records = mapBybitTransactionToIncomeRecords(row());

    const commission = records.find((r) => r.category === 'commission');
    expect(commission).toBeDefined();
    // Charged fee reduces equity, so it must be reported negative.
    expect(commission!.income).toBeCloseTo(-0.0055, 10);
  });

  it('splits pnl, funding and fee into separate records with distinct tranIds', () => {
    const records = mapBybitTransactionToIncomeRecords(row({ funding: '-0.013' }));

    expect(records).toHaveLength(3);
    expect(records.map((r) => r.category).sort()).toEqual(['commission', 'funding', 'realized_pnl']);
    expect(new Set(records.map((r) => r.tranId)).size).toBe(3);
  });

  it('sums to bybit cashFlow + funding - fee', () => {
    const record = row({ funding: '-0.013' });
    const total = mapBybitTransactionToIncomeRecords(record).reduce((acc, r) => acc + r.income, 0);

    expect(total).toBeCloseTo(parseFloat(record.cashFlow) + parseFloat(record.funding) - parseFloat(record.fee), 10);
  });

  it('reports a negative fee as a rebate', () => {
    const records = mapBybitTransactionToIncomeRecords(row({ fee: '-0.002', cashFlow: '0' }));

    expect(records).toHaveLength(1);
    expect(records[0].category).toBe('rebate');
    expect(records[0].income).toBeCloseTo(0.002, 10);
  });

  it('categorises settlement rows as funding', () => {
    const records = mapBybitTransactionToIncomeRecords(
      row({ type: 'SETTLEMENT' as BybitTransactionRecord['type'], cashFlow: '-0.09', fee: '0' }),
    );

    expect(records).toHaveLength(1);
    expect(records[0].category).toBe('funding');
  });
});
