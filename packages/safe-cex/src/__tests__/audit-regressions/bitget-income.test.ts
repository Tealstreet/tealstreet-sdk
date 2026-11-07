import { describe, expect, it } from 'vitest';

import { mapBitgetBill, mapBitgetBills } from '../reference/bitget/bitget.income';

/**
 * Oracles are the exact rows measured live on sheet row 36 (819 rows / 90 days), not
 * invented shapes. Bitget is a SEVENTH convention profile and each assertion below pins one
 * way it differs from the six already measured.
 */

/** A closing row: gross PnL in `amount`, fee NEGATIVE alongside it. */
const closeLong = {
  billId: '1439829381856505906',
  symbol: 'TRXUSDT',
  amount: '-0.00084',
  fee: '-0.00595324',
  feeByCoupon: '',
  businessType: 'close_long',
  coin: 'USDT',
  balance: '5.89271701',
  cTime: '1779007996047',
};

/** Funding. Its own row type, carried in `amount`, with `fee` always 0. */
const funding = {
  billId: '1442715095300452415',
  symbol: 'TRXUSDT',
  amount: '-0.00099009',
  fee: '0',
  businessType: 'contract_settle_fee',
  coin: 'USDT',
  balance: '4.80335792',
  cTime: '1779696003746',
};

describe('mapBitgetBill', () => {
  it('treats `amount` as GROSS and emits the fee as a SEPARATE record', () => {
    // Pinned by the row's own running balance: consecutive deltas matched `amount + fee` on
    // 676 of 818 comparable rows and matched `amount` alone on 6. Folding the fee into
    // realized PnL would understate every closing row by its commission while the account
    // total still reconciled.
    const records = mapBitgetBill(closeLong);
    const realized = records.find((r) => r.category === 'realized_pnl');
    const commission = records.find((r) => r.category === 'commission');

    expect(realized?.income).toBeCloseTo(-0.00084, 12);
    expect(commission?.income).toBeCloseTo(-0.00595324, 12);
    // NOT amount + fee.
    expect(realized?.income).not.toBeCloseTo(-0.00084 + -0.00595324, 12);
  });

  it('passes the fee sign through UNCHANGED — bitget already signs it negative', () => {
    // This is the field blofin and bitunix disagree on. Negating it here would double the
    // error rather than fix it, and no other invariant would see it.
    const commission = mapBitgetBill(closeLong).find((r) => r.category === 'commission');
    expect(commission!.income).toBeLessThan(0);
  });

  it('categorises contract_settle_fee as FUNDING, not realized PnL', () => {
    // okx, bybit and bitunix all fold funding into realized PnL and had to be caught doing
    // it. Bitget keeps it separate — mapping it to realized_pnl would silently reproduce
    // their defect on a venue that does not have it.
    const records = mapBitgetBill(funding);
    expect(records).toHaveLength(1);
    expect(records[0]).toMatchObject({ category: 'funding', symbol: 'TRXUSDT' });
    expect(records[0].income).toBeCloseTo(-0.00099009, 12);
  });

  it('emits no realized-PnL record for an opening row', () => {
    // open_* rows carry `amount: 0` and exist only to charge the fee. A zero-income realized
    // record would be noise in every census that counts them.
    const open = { ...closeLong, businessType: 'open_long', amount: '0' };
    const records = mapBitgetBill(open);
    expect(records.map((r) => r.category)).toEqual(['commission']);
  });

  it('handles the one-way spelling (buy/sell) identically to open_/close_', () => {
    // The sampled account contains both, because position mode changes the spelling rather
    // than the semantics. Handling only one set would drop half the ledger.
    const sell = { ...closeLong, businessType: 'sell' };
    const records = mapBitgetBill(sell);
    expect(records.map((r) => r.category).sort()).toEqual(['commission', 'realized_pnl']);
  });

  it('maps a transfer and carries NO symbol for it', () => {
    const transfer = {
      billId: '1470632679286931457',
      symbol: '',
      amount: '100',
      fee: '0',
      businessType: 'trans_from_exchange',
      coin: 'USDT',
      balance: '101.00859246',
      cTime: '1786352074449',
    };
    const records = mapBitgetBill(transfer);
    expect(records).toHaveLength(1);
    expect(records[0]).toMatchObject({ category: 'transfer', symbol: '', income: 100 });
  });

  it('emits an UNKNOWN businessType as `other` rather than dropping it', () => {
    // Dropping shrinks the ledger side of the identity silently. Mapped to `other` the row
    // still appears with its raw type, so a new bitget event class surfaces as an
    // unexplained residual WITH A NAME rather than as a hole.
    const records = mapBitgetBill({ ...closeLong, businessType: 'some_new_bitget_thing' });
    expect(records).toHaveLength(1);
    expect(records[0].category).toBe('other');
    expect(records[0].incomeType).toBe('some_new_bitget_thing');
  });

  it('drops a row with no usable timestamp instead of dating it to the epoch', () => {
    expect(mapBitgetBill({ ...closeLong, cTime: '' })).toHaveLength(0);
    expect(mapBitgetBill({ ...closeLong, cTime: '0' })).toHaveLength(0);
  });

  it('SUFFIXES tranId per record, so a bill yielding two records has two ids', () => {
    // A bare billId is not unique per record. Deduping on it discarded the commission from
    // every trade — 820 records where the census predicted ~1190 — and the live paging probe
    // could not see it, because both walks lost the same rows. Five other venues already
    // suffix; bitget and bitmex were the outliers.
    const ids = mapBitgetBill(closeLong).map((r) => r.tranId);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids.sort()).toEqual(['1439829381856505906:fee', '1439829381856505906:pnl']);
  });

  it('maps a batch and keeps every row', () => {
    const records = mapBitgetBills([closeLong, funding]);
    // 2 from the close (pnl + fee) + 1 funding.
    expect(records).toHaveLength(3);
  });
});
