import { describe, expect, it } from 'vitest';

import { mapBitunixPositionHistory } from '../../journal/bitunix.position-adapter';
import { PositionSide } from '../../types';

const deps = { account: 'test', resolveSettleCurrency: () => 'USDT' };

/** Measured live: a flat round trip, entryPrice == closePrice, funding 0. */
const flatRow = {
  positionId: '7301849293375090406',
  symbol: 'LINKUSDT',
  marginCoin: null,
  maxQty: '0.23',
  qty: '0.23',
  entryPrice: '8.767',
  closePrice: '8.767',
  liqQty: null,
  side: 'SELL',
  marginMode: 'CROSS',
  positionMode: 'HEDGE',
  leverage: '10',
  fee: '0.00201641',
  funding: '0',
  realizedPNL: '-0.00201641',
  ctime: '1786523399000',
  mtime: '1786523412000',
};

/** Measured live: the account's REAL liquidation, with non-zero funding. */
const liquidationRow = {
  positionId: '9071536168626357642',
  symbol: 'LINKUSDT',
  maxQty: '7.02',
  qty: '7.02',
  entryPrice: '9.3440897435897435',
  closePrice: '8.235',
  liqQty: '-4.6685219852125676',
  side: 'BUY',
  marginMode: 'CROSS',
  positionMode: 'HEDGE',
  leverage: '10',
  fee: '0.048387159',
  funding: '-0.0602176186710414',
  realizedPNL: '-6.1898547776710403',
  liqPrice: '8.235',
  ctime: '1779262461000',
  mtime: '1780457988000',
};

describe('bitunix position adapter', () => {
  it('reaches gross by ADDING a positive fee back', () => {
    // `fee` is POSITIVE when charged here — the opposite of blofin and okx. Negating it, as
    // those venues need, would flip every fee into a rebate. Pinned by this row: the price
    // did not move and funding is zero, so gross must be exactly 0.
    const record = mapBitunixPositionHistory(flatRow, deps)!;
    expect(record.fees).toBeCloseTo(0.00201641, 10);
    expect(record.realizedPnl).toBeCloseTo(0, 10);
  });

  it('reads `side` as the POSITION direction, not an order side', () => {
    // Verified on 14 live rows with price movement: every SELL profits when the price falls,
    // every BUY when it rises, zero contradictions. Reading this as an order side would
    // invert every record while still reconciling in aggregate.
    expect(mapBitunixPositionHistory(flatRow, deps)!.side).toBe(PositionSide.Short);
    expect(mapBitunixPositionHistory({ ...flatRow, side: 'BUY' }, deps)!.side).toBe(PositionSide.Long);
  });

  it('refuses a row whose side it cannot read', () => {
    expect(mapBitunixPositionHistory({ ...flatRow, side: '' }, deps)).toBeNull();
  });

  it('classifies the account REAL liquidation from liqQty', () => {
    // Not a synthetic fixture: this is the actual liquidated position on sheet row 56,
    // closePrice == liqPrice == 8.235. It is the only venue-emitted liquidation observed
    // anywhere in this epic.
    const record = mapBitunixPositionHistory(liquidationRow, deps)!;
    expect(record.closeReason).toBe('liquidation');
    expect(mapBitunixPositionHistory(flatRow, deps)!.closeReason).toBe('user');
  });

  it('carries venue-reported funding on the position', () => {
    // bybit and blofin report no funding on the closed row at all; this one does, so it is
    // attributed by the venue rather than allocated by us.
    const record = mapBitunixPositionHistory(liquidationRow, deps)!;
    expect(record.funding).toBeCloseTo(-0.0602176186710414, 12);
    expect(record.fundingAttributed).toBe(true);
  });

  it('removes funding from realizedPNL when told to — which live measurement says it must', () => {
    // MEASURED: a 90-day identity run with two funding-bearing positions failed by
    // -0.09159256 against a funding total of -0.09159256, so realizedPNL IS net of funding
    // and callers must pass the flag. A 30-day window has zero funding and reconciles fine
    // without it — the false green this asserts against.
    const asIs = mapBitunixPositionHistory(liquidationRow, deps)!;
    const netOfFunding = mapBitunixPositionHistory(liquidationRow, { ...deps, realizedIsNetOfFunding: true })!;
    expect(netOfFunding.realizedPnl - asIs.realizedPnl).toBeCloseTo(0.0602176186710414, 12);
  });

  it('keeps peak size and closed volume separate', () => {
    const record = mapBitunixPositionHistory({ ...flatRow, maxQty: '5', qty: '2' }, deps)!;
    expect(record.closedSize).toBe(2);
    expect(record.maxSize).toBe(5);
  });

  it('reads the hedge leg from the explicit positionMode field', () => {
    expect(mapBitunixPositionHistory(flatRow, deps)!.positionSide).toBe(PositionSide.Short);
    // One-way mode still has a direction, but no leg to occupy.
    expect(mapBitunixPositionHistory({ ...flatRow, positionMode: 'ONE_WAY' }, deps)!.positionSide).toBe('both');
  });

  it('carries a real open time', () => {
    const record = mapBitunixPositionHistory(flatRow, deps)!;
    expect(record.openedAt).toBe(1786523399000);
    expect(record.closedAt).toBe(1786523412000);
  });
});
