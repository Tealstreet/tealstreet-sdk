import { describe, expect, it } from 'vitest';

import { mapBitunixClosedPosition, mapBitunixTradeToIncomeRecords } from '../../exchanges/bitunix/bitunix.income';
import { mapBlofinClosedPosition, mapBlofinFillToIncomeRecords } from '../../exchanges/blofin/blofin.income';
import { PositionSide } from '../../types';

const resolveBlofin = (instId: string) => (instId === 'DOGE-USDT' ? 'DOGEUSDT' : undefined);

/** Shapes captured live: blofin sheet row 41, bitunix sheet row 56. */
const blofinFill = (over: Record<string, any> = {}) => ({
  instId: 'DOGE-USDT',
  tradeId: '1000104108988',
  orderId: '1000164597347',
  fillPrice: '0.07169',
  fillSize: '0.21',
  fillPnl: '-0.0021',
  side: 'sell',
  positionSide: 'long',
  fee: '0.008280195',
  ts: '1786521923361',
  ...over,
});

const blofinPosition = (over: Record<string, any> = {}) => ({
  historyId: '114948331',
  positionId: '1000000611218',
  instId: 'DOGE-USDT',
  positionSide: 'long',
  closePositions: '420',
  openAveragePrice: '0.0717',
  closeAveragePrice: '0.07169',
  createTime: '1786521873701',
  updateTime: '1786521923388',
  leverage: '10',
  realizedPnl: '-0.03732309',
  fee: '-0.03312309',
  ...over,
});

const bitunixTrade = (over: Record<string, any> = {}) => ({
  tradeId: '6818636119215190295',
  symbol: 'LINKUSDT',
  qty: '0.23',
  price: '8.767',
  side: 'BUY',
  fee: '0.001008205',
  realizedPNL: '0.5',
  ctime: '1786523412000',
  ...over,
});

const bitunixPosition = (over: Record<string, any> = {}) => ({
  positionId: '7301849293375090406',
  symbol: 'LINKUSDT',
  qty: '0.23',
  entryPrice: '8.767',
  closePrice: '8.767',
  side: 'SELL',
  leverage: '10',
  fee: '0.00201641',
  funding: '0',
  realizedPNL: '-0.00201641',
  ctime: '1786523399000',
  mtime: '1786523412000',
  ...over,
});

describe('blofin — the two endpoints use OPPOSITE fee signs', () => {
  it('fills: fee is positive when charged, so income is negative', () => {
    const commission = mapBlofinFillToIncomeRecords(blofinFill(), resolveBlofin).find(
      (r) => r.category === 'commission',
    );
    expect(commission!.income).toBeCloseTo(-0.008280195, 12);
  });

  it('positions: fee arrives NEGATIVE when charged, and `fees` means fees paid', () => {
    // Normalising both endpoints to one sign at the call site is exactly the mistake this
    // guards: each mapper owns its own convention.
    expect(mapBlofinClosedPosition(blofinPosition(), resolveBlofin, 'acct')!.fees).toBeCloseTo(0.03312309, 12);
  });

  it('positions: realizedPnl is NET of fee, so gross recovers the price move', () => {
    const entry = mapBlofinClosedPosition(blofinPosition(), resolveBlofin, 'acct')!;
    // 420 * (0.07169 - 0.0717) = -0.0042
    expect((entry.unrealizedPnl as number) + (entry.fees as number)).toBeCloseTo(-0.0042, 8);
  });

  it('a negative fill fee is a rebate, not an absolute-valued commission', () => {
    const records = mapBlofinFillToIncomeRecords(blofinFill({ fee: '-0.002', fillPnl: '0' }), resolveBlofin);
    expect(records).toHaveLength(1);
    expect(records[0].category).toBe('rebate');
  });

  it('keys tranId on tradeId / historyId so identical rows do not collapse', () => {
    const a = mapBlofinFillToIncomeRecords(blofinFill({ tradeId: 'a' }), resolveBlofin);
    const b = mapBlofinFillToIncomeRecords(blofinFill({ tradeId: 'b' }), resolveBlofin);
    expect(a[0].tranId).not.toBe(b[0].tranId);
  });

  it('falls back to the raw instId rather than dropping an unknown market', () => {
    expect(mapBlofinFillToIncomeRecords(blofinFill({ instId: 'NEW-USDT' }), resolveBlofin)[0].symbol).toBe('NEW-USDT');
  });
});

describe('bitunix', () => {
  it('realizedPNL is NET of fee — gross recovers a zero price move', () => {
    // entryPrice == closePrice, so gross is 0 and realizedPNL must be exactly -fee.
    const entry = mapBitunixClosedPosition(bitunixPosition(), 'acct')!;
    expect((entry.unrealizedPnl as number) + (entry.fees as number)).toBeCloseTo(0, 10);
  });

  it('does NOT invert side — `side` is the position, not a closing order', () => {
    // bybit/binance report the CLOSING order's side and must be inverted; bitunix does not.
    expect(mapBitunixClosedPosition(bitunixPosition({ side: 'SELL' }), 'acct')!.side).toBe(PositionSide.Short);
    expect(mapBitunixClosedPosition(bitunixPosition({ side: 'BUY' }), 'acct')!.side).toBe(PositionSide.Long);
  });

  it('emits pnl and fee terms from one trade row', () => {
    const records = mapBitunixTradeToIncomeRecords(bitunixTrade());
    expect(records.map((r) => r.category).sort()).toEqual(['commission', 'realized_pnl']);
    expect(records.find((r) => r.category === 'commission')!.income).toBeCloseTo(-0.001008205, 12);
  });

  it('sums to realizedPNL - fee, so the ledger conserves', () => {
    const total = mapBitunixTradeToIncomeRecords(bitunixTrade()).reduce((acc, r) => acc + r.income, 0);
    expect(total).toBeCloseTo(0.5 - 0.001008205, 12);
  });

  it('drops rows with no symbol or timestamp', () => {
    expect(mapBitunixTradeToIncomeRecords(bitunixTrade({ symbol: '' }))).toHaveLength(0);
    expect(mapBitunixClosedPosition(bitunixPosition({ ctime: '0' }), 'acct')).toBeNull();
  });

  it('stamps venue provenance so it can cross-check the ledger', () => {
    expect(mapBitunixClosedPosition(bitunixPosition(), 'acct')!.provenance).toBe('venue');
  });
});
