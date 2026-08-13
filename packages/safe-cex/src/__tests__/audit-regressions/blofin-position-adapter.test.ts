import { describe, expect, it } from 'vitest';

import { mapBlofinPositionHistory } from '../../journal/blofin.position-adapter';
import { PositionSide } from '../../types';

const deps = {
  account: 'test',
  resolveSymbol: (instId: string) => (instId === 'DOGE-USDT' ? 'DOGEUSDT' : undefined),
  resolveSettleCurrency: () => 'USDT',
};

/** The exact row measured live on sheet row 39. */
const row = {
  historyId: '114948331',
  positionId: '1000000611218',
  instId: 'DOGE-USDT',
  instType: 'SWAP',
  marginMode: 'cross',
  positionSide: 'long',
  closePositions: '420',
  maxPositions: '420',
  liquidationPositions: '0',
  openAveragePrice: '0.0717',
  closeAveragePrice: '0.07169',
  createTime: '1786521873701',
  updateTime: '1786521923388',
  leverage: '10',
  realizedPnl: '-0.03732309',
  realizedPnlRatio: '-0.012393933054393305',
  fee: '-0.03312309',
};

describe('blofin position adapter', () => {
  it('reaches gross by SUBTRACTING a negative fee, not by absolute value', () => {
    // `fee` is negative when charged (okx's sign convention, not binance's), so
    // gross = realizedPnl - fee. Measured across rows whose pnl and fee both varied, this
    // lands on a consistent -0.0042.
    const record = mapBlofinPositionHistory(row, deps)!;
    expect(record.fees).toBeCloseTo(0.03312309, 10);
    expect(record.realizedPnl).toBeCloseTo(-0.0042, 8);
  });

  it('treats a positive fee as a REBATE rather than a cost', () => {
    // Math.abs here would report a rebate as a cost AND add it with the wrong sign — an
    // error of twice the rebate that reconciles in aggregate and appears only on maker fills.
    const record = mapBlofinPositionHistory({ ...row, fee: '0.001', realizedPnl: '0.001' }, deps)!;
    expect(record.fees).toBeCloseTo(-0.001, 10);
    expect(record.realizedPnl).toBeCloseTo(0, 10);
  });

  it('carries a REAL open time, unlike bybit', () => {
    const record = mapBlofinPositionHistory(row, deps)!;
    expect(record.openedAt).toBe(1786521873701);
    expect(record.closedAt).toBe(1786521923388);
    expect(record.closedAt! - record.openedAt!).toBeGreaterThan(0);
  });

  it('keeps peak size and closed volume as separate fields', () => {
    // They diverge whenever a position is added to mid-life; conflating them reports a size
    // the position never held.
    const record = mapBlofinPositionHistory({ ...row, maxPositions: '600' }, deps)!;
    expect(record.closedSize).toBe(420);
    expect(record.maxSize).toBe(600);
  });

  it('takes the hedge leg straight off the row', () => {
    expect(mapBlofinPositionHistory(row, deps)!.positionSide).toBe(PositionSide.Long);
    expect(mapBlofinPositionHistory({ ...row, positionSide: 'short' }, deps)!.positionSide).toBe(PositionSide.Short);
  });

  it('REFUSES a one-way `net` row rather than guessing its direction', () => {
    // `closePositions` is unsigned and the prices cannot distinguish a winning long from a
    // losing short once fees are netted in. Guessing would silently invert half a one-way
    // account's history; the caller counts the refusal as skipped instead.
    expect(mapBlofinPositionHistory({ ...row, positionSide: 'net' }, deps)).toBeNull();
  });

  it('keys on historyId, because positionId is reused across cycles', () => {
    // A live sample had 20 rows across only 2 positionIds — the same trap as okx's posId.
    const a = mapBlofinPositionHistory(row, deps)!;
    const b = mapBlofinPositionHistory({ ...row, historyId: '114948332' }, deps)!;
    expect(a.key).not.toBe(b.key);
  });

  it('classifies a liquidation from a non-zero liquidated quantity', () => {
    expect(mapBlofinPositionHistory({ ...row, liquidationPositions: '420' }, deps)!.closeReason).toBe('liquidation');
    expect(mapBlofinPositionHistory(row, deps)!.closeReason).toBe('user');
  });

  it('leaves funding at zero and unattributed, because the row has no funding field', () => {
    const record = mapBlofinPositionHistory(row, deps)!;
    expect(record.funding).toBe(0);
    expect(record.fundingAttributed).toBe(false);
  });

  it('applies a contract size only when the venue quotes contracts', () => {
    // Verified BASE on DOGE-USDT (420 against a 420-DOGE ladder), so the default is a
    // no-op — but okx quotes the same instrument family in CONTRACTS, and getting that
    // wrong is invisible downstream.
    const scaled = mapBlofinPositionHistory(row, { ...deps, contractSize: () => 10 })!;
    expect(scaled.closedSize).toBe(4200);
  });
});
