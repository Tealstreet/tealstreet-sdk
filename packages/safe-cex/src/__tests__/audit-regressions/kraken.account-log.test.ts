import { describe, expect, it } from 'vitest';

import { mapKrakenAccountLog, mapKrakenAccountLogRow } from '../../exchanges/kraken/kraken.income';

/**
 * Kraken Futures `history/v3/account-log` -> `IncomeRecord[]`.
 *
 * Every row below is transcribed from a live probe of sheet row 65. The venue was recorded
 * as having no history axis at all; it has the richest one in the epic, on a path prefix the
 * client could not reach.
 */

const resolveSymbol = (contract: string) => (contract.toLowerCase() === 'pf_xrpusd' ? 'XRPUSD' : undefined);

/** Cash leg of a futures trade — fee and PnL live here. */
const cashLeg = {
  asset: 'usd',
  contract: 'pf_xrpusd',
  booking_uid: '5daa4b1f-fd82-4007-99d8-476f5bf9d9a2',
  date: '2026-08-12T10:19:53.188Z',
  execution: 'd19f075f-78f4-4d3e-88b0-7f8b2a8ea287',
  fee: 0.0002,
  id: 8690,
  info: 'futures trade',
  new_balance: 237.4814,
  old_balance: 237.4817,
  realized_funding: -0.0001,
  realized_pnl: 0,
};

/** Its twin, sharing one `execution`. `old_balance`/`new_balance` here are CONTRACTS. */
const positionSizeLeg = {
  asset: 'pf_xrpusd',
  contract: 'pf_xrpusd',
  booking_uid: 'e061c626-e919-4259-a767-ff2792be2139',
  date: '2026-08-12T10:19:53.188Z',
  execution: 'd19f075f-78f4-4d3e-88b0-7f8b2a8ea287',
  fee: null,
  id: 8689,
  info: 'futures trade',
  new_balance: 196,
  old_balance: 195,
  realized_funding: null,
  realized_pnl: null,
};

const fundingRow = {
  asset: 'usd',
  contract: 'pf_xrpusd',
  booking_uid: '40be32b6-f261-4690-b21b-251c5b9e895d',
  date: '2026-08-14T05:00:00.000Z',
  execution: null,
  fee: 0,
  id: 8776,
  info: 'funding rate change',
  new_balance: 237.2536,
  old_balance: 237.2573,
  realized_funding: -0.0037,
  realized_pnl: null,
};

describe('mapKrakenAccountLogRow', () => {
  it('DROPS the position-size leg, whose balance is contracts rather than currency', () => {
    // 195 -> 196 is a one-contract fill. Mapping it would enter a position size into a
    // currency total, on top of double-counting the trade.
    expect(mapKrakenAccountLogRow(positionSizeLeg, resolveSymbol)).toEqual([]);
  });

  it('keeps the cash leg of the same execution', () => {
    const records = mapKrakenAccountLogRow(cashLeg, resolveSymbol);
    expect(records.length).toBeGreaterThan(0);
    expect(records[0].tradeId).toBe('d19f075f-78f4-4d3e-88b0-7f8b2a8ea287');
  });

  it('NEGATES the fee, because kraken reports it positive when charged', () => {
    // woo's sign, the opposite of okx and blofin — whose commission arrives already negative
    // and must be passed through. Getting this backwards reconciles in aggregate while every
    // fee is doubled in the wrong direction.
    const fee = mapKrakenAccountLogRow(cashLeg, resolveSymbol).find((r) => r.category === 'commission');
    expect(fee?.income).toBe(-0.0002);
  });

  it('passes realized_funding through, since it is already negative when paid', () => {
    const funding = mapKrakenAccountLogRow(fundingRow, resolveSymbol).find((r) => r.category === 'funding');
    expect(funding?.income).toBe(-0.0037);
  });

  it('does not emit a PnL record for a genuine zero, nor for a null', () => {
    // An opening trade realizes exactly 0 and a funding row reports null. Neither is a PnL
    // event; emitting either would put a zero-value row into the ledger.
    expect(mapKrakenAccountLogRow(cashLeg, resolveSymbol).some((r) => r.category === 'realized_pnl')).toBe(false);
    expect(mapKrakenAccountLogRow(fundingRow, resolveSymbol).some((r) => r.category === 'realized_pnl')).toBe(false);
  });

  it('emits a PnL record when one is actually realized', () => {
    const closing = { ...cashLeg, realized_pnl: -1.234, booking_uid: 'close-1' };
    const pnl = mapKrakenAccountLogRow(closing, resolveSymbol).find((r) => r.category === 'realized_pnl');
    expect(pnl).toMatchObject({ income: -1.234, symbol: 'XRPUSD', asset: 'USD' });
  });

  it('parses the ISO-8601 date rather than passing it through', () => {
    // Every other venue on this axis reports epoch millis. Unparsed, this lands in 1970 and
    // a window filter then discards the whole account — an empty axis, not a wrong date.
    expect(mapKrakenAccountLogRow(fundingRow, resolveSymbol)[0].timestamp).toBe(Date.parse('2026-08-14T05:00:00.000Z'));
  });

  it('gives each component of one row a DISTINCT tranId', () => {
    // The bitget lesson: a shared id collapses half the ledger at the dedupe step, and the
    // paging probe still passes because both walks lose the same rows.
    const ids = mapKrakenAccountLogRow(cashLeg, resolveSymbol).map((r) => r.tranId);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('records an unknown entry type as `other` rather than dropping it', () => {
    // Only `futures trade` and `funding rate change` appeared in the sampled window, so the
    // vocabulary is known to be incomplete. A balance moved, so value moved.
    const unknown = {
      ...cashLeg,
      booking_uid: 'unknown-1',
      // Deliberately NOT a transfer word — those now classify as `transfer`. This asserts
      // the genuinely-unrecognised case still lands in `other` rather than being dropped.
      info: 'settlement adjustment',
      fee: null,
      realized_pnl: null,
      realized_funding: null,
      old_balance: 100,
      new_balance: 150,
    };
    expect(mapKrakenAccountLogRow(unknown, resolveSymbol)).toEqual([
      expect.objectContaining({ category: 'other', income: 50, incomeType: 'settlement adjustment' }),
    ]);
  });

  it('classifies a capital movement as `transfer`, not `other`', () => {
    // Measured live: `admin transfer` worth 95.07900000. Left in `other` it sat in the same
    // bucket as an unclassified anomaly, and a total should never absorb an unexplained row.
    const transfer = {
      ...cashLeg,
      booking_uid: 'admin-1',
      info: 'admin transfer',
      contract: null,
      fee: null,
      realized_pnl: null,
      realized_funding: null,
      old_balance: 100,
      new_balance: 195.079,
    };
    expect(mapKrakenAccountLogRow(transfer, resolveSymbol)).toEqual([
      expect.objectContaining({ category: 'transfer', incomeType: 'admin transfer' }),
    ]);
  });

  it('every record for one row reconciles to that row balance delta', () => {
    // The balance oracle, as a unit. It is what killed the triple-count hypothesis on live
    // data (347/347 rows reconciled) and it pins gross-vs-net, the fee sign and whether
    // funding is separate — arithmetically rather than by inspection.
    for (const row of [cashLeg, fundingRow]) {
      const delta = Number(row.new_balance) - Number(row.old_balance);
      const mapped = mapKrakenAccountLogRow(row, resolveSymbol).reduce((acc, r) => acc + Number(r.income), 0);
      expect(mapped).toBeCloseTo(delta, 8);
    }
  });

  it('drops a row that moved nothing and carries no components', () => {
    const inert = {
      ...cashLeg,
      booking_uid: 'inert',
      fee: 0,
      realized_pnl: 0,
      realized_funding: 0,
      old_balance: 5,
      new_balance: 5,
    };
    expect(mapKrakenAccountLogRow(inert, resolveSymbol)).toEqual([]);
  });

  it('falls back to the raw contract when the market is unknown', () => {
    const other = { ...cashLeg, contract: 'pf_solusd', booking_uid: 'sol-1', realized_pnl: 1 };
    expect(mapKrakenAccountLogRow(other, resolveSymbol)[0].symbol).toBe('pf_solusd');
  });

  it('maps a whole page, dropping only the size legs', () => {
    const records = mapKrakenAccountLog([cashLeg, positionSizeLeg, fundingRow], resolveSymbol);
    expect(records.every((r) => r.asset === 'USD')).toBe(true);
    expect(records.some((r) => r.category === 'funding')).toBe(true);
  });
});
