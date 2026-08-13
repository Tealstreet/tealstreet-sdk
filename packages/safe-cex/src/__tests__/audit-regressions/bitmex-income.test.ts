import { describe, expect, it } from 'vitest';

import { mapBitmexWalletHistory } from '../../exchanges/bitmex/bitmex.income';

/** USDt is 1e6 minor units, XBt is satoshis at 1e8 — measured on the same response. */
const deps = { scaleFor: (c: string) => (c === 'USDt' ? 1e6 : c === 'XBt' ? 1e8 : undefined) };

/** The exact shape measured live on sheet row 43. */
const pnlRow = {
  transactID: '00000000-0077-1000-0000-00008429ccc7',
  currency: 'USDt',
  transactType: 'RealisedPNL',
  amount: -124168,
  walletBalance: 60897115,
  fee: 6568,
  transactStatus: 'Completed',
  address: 'XRPUSDT',
  transactTime: '2026-08-06T09:00:00.000Z',
};

describe('mapBitmexWalletHistory', () => {
  it('scales minor units per currency, not with one constant', () => {
    // The trap this venue exists to demonstrate: USDt and XBt arrive in the SAME response
    // with different scales, so a single divisor is wrong for half the rows.
    const [usdt] = mapBitmexWalletHistory([pnlRow], deps);
    const [xbt] = mapBitmexWalletHistory([{ ...pnlRow, currency: 'XBt', amount: -124168, fee: 6568 }], deps);

    expect(usdt.income).toBeCloseTo((-124168 + 6568) / 1e6, 12);
    expect(xbt.income).toBeCloseTo((-124168 + 6568) / 1e8, 12);
    // 100x apart from identical raw numbers.
    expect(Math.abs(usdt.income / xbt.income)).toBeCloseTo(100, 6);
  });

  it('DROPS a currency whose scale is unknown rather than guessing', () => {
    // A wrong power of ten reconciles against a ledger scaled the same way and passes every
    // invariant except the magnitude check. Dropping is the honest failure.
    expect(mapBitmexWalletHistory([{ ...pnlRow, currency: 'WEIRD' }], deps)).toHaveLength(0);
  });

  it('treats `amount` as NET of fee and emits the gross PnL term', () => {
    // Pinned by a measured break-even row where the entire loss IS the commission:
    // amount -6594 against fee 6594.
    const records = mapBitmexWalletHistory([{ ...pnlRow, amount: -6594, fee: 6594 }], deps);
    const realized = records.find((r) => r.category === 'realized_pnl');
    const commission = records.find((r) => r.category === 'commission');

    // Gross is exactly zero — the trade broke even and the fee was the whole loss.
    expect(realized).toBeUndefined();
    expect(commission?.income).toBeCloseTo(-6594 / 1e6, 12);
  });

  it('splits one row into separate realized-PnL and commission records, with DISTINCT ids', () => {
    const records = mapBitmexWalletHistory([pnlRow], deps);
    expect(records.map((r) => r.category).sort()).toEqual(['commission', 'realized_pnl']);
    // The bare transactID is not unique per record; a consumer deduping on `tranId` would
    // keep one of the two. Measured on bitget, where that discarded every commission.
    const ids = records.map((r) => r.tranId);
    expect(new Set(ids).size).toBe(ids.length);
    // Commission is NEGATIVE when paid, matching every other venue's ledger sign.
    expect(records.find((r) => r.category === 'commission')!.income).toBeLessThan(0);
  });

  it('reads the symbol out of `address`, which is where BitMEX puts it', () => {
    expect(mapBitmexWalletHistory([pnlRow], deps)[0].symbol).toBe('XRPUSDT');
  });

  it('does not treat a wallet address as a symbol on transfers', () => {
    const [transfer] = mapBitmexWalletHistory(
      [{ ...pnlRow, transactType: 'Deposit', address: '3BMEXaddressNotASymbol', fee: 0, amount: 5_000_000 }],
      deps,
    );
    expect(transfer.category).toBe('transfer');
    expect(transfer.symbol).toBe('');
    expect(transfer.income).toBeCloseTo(5, 12);
  });

  it('ignores rows that are not Completed', () => {
    // A pending withdrawal has not left the account; counting it would make the ledger
    // disagree with the balance.
    expect(mapBitmexWalletHistory([{ ...pnlRow, transactStatus: 'Pending' }], deps)).toHaveLength(0);
  });

  it('drops transaction types it does not recognise rather than miscategorising them', () => {
    expect(mapBitmexWalletHistory([{ ...pnlRow, transactType: 'SomeNewThing' }], deps)).toHaveLength(0);
  });
});
