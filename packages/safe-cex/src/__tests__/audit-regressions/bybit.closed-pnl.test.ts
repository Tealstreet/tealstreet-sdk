import { describe, expect, it } from 'vitest';

import { mapBybitClosedPnl } from '../../exchanges/bybit/bybit.closed-pnl';
import { PositionSide } from '../../types';

/**
 * The consumer casts this result `as unknown[] as ClosedPosition[]`, so TypeScript cannot
 * catch a single shape mistake here. Every assertion below covers something that would
 * typecheck cleanly and be wrong at runtime.
 */
const row = (over: Record<string, any> = {}) => ({
  symbol: 'DOGEUSDT',
  orderId: 'order-1',
  side: 'Sell',
  qty: '100',
  closedSize: '100',
  avgEntryPrice: '0.1',
  avgExitPrice: '0.12',
  cumEntryValue: '10',
  cumExitValue: '12',
  closedPnl: '1.98',
  openFee: '0.011',
  closeFee: '0.0132',
  leverage: '10',
  createdTime: '1754000000000',
  updatedTime: '1754000060000',
  ...over,
});

describe('bybit closed-pnl mapping', () => {
  it('inverts the side, because bybit reports the CLOSING order side', () => {
    // A Sell that closed the position means the position was Long. Getting this backwards
    // flips every row in the UI and still compiles.
    expect(mapBybitClosedPnl(row({ side: 'Sell' }), 'acct')!.side).toBe(PositionSide.Long);
    expect(mapBybitClosedPnl(row({ side: 'Buy' }), 'acct')!.side).toBe(PositionSide.Short);
  });

  it('coerces every numeric, because the grid subtracts and divides them', () => {
    const entry = mapBybitClosedPnl(row(), 'acct')!;

    for (const field of ['notional', 'contracts', 'entryPrice', 'exitPrice', 'unrealizedPnl', 'fees', 'leverage']) {
      expect(typeof (entry as any)[field]).toBe('number');
    }
    expect(entry.entryPrice).toBe(0.1);
    expect(entry.unrealizedPnl).toBe(1.98);
  });

  it('sums the real open and close fees rather than reporting zero', () => {
    // The dead route this replaces hardcoded `fees: 0`.
    expect(mapBybitClosedPnl(row(), 'acct')!.fees).toBeCloseTo(0.0242, 10);
  });

  it('stamps the account, which the Account column and SymbolDisplay read', () => {
    expect(mapBybitClosedPnl(row(), 'my-account')!.account).toBe('my-account');
  });

  it('falls back to createdTime when updatedTime is absent', () => {
    const entry = mapBybitClosedPnl(row({ updatedTime: undefined }), 'acct')!;
    expect(entry.closedTime).toBe(1754000000000);
  });

  it('converts inverse sizes to the same units mapPosition uses', () => {
    // On inverse, bybit's `closedSize` is USD contracts and `cumExitValue` is coin —
    // the opposite of linear. `BybitInverseExchange.mapPosition` stores contracts in coin
    // and notional in USD, and the grid renders closed rows next to open ones, so a raw
    // passthrough shows one trade as "1000 BTC" here and "0.01 BTC" there.
    const inverseRow = row({
      symbol: 'BTCUSD',
      closedSize: '1000',
      cumExitValue: '0.01',
      avgExitPrice: '100000',
    });

    const linear = mapBybitClosedPnl(inverseRow, 'acct', false)!;
    expect(linear.contracts).toBe(1000);
    expect(linear.notional).toBe(0.01);

    const inverse = mapBybitClosedPnl(inverseRow, 'acct', true)!;
    expect(inverse.contracts).toBe(0.01); // coin
    expect(inverse.notional).toBe(1000); // USD
  });

  it('does not divide by a zero exit price on inverse', () => {
    const entry = mapBybitClosedPnl(row({ avgExitPrice: '0', closedSize: '1000' }), 'acct', true)!;
    expect(entry.contracts).toBe(1000);
    expect(Number.isFinite(entry.contracts as number)).toBe(true);
  });

  it('drops a row with no symbol or no timestamp rather than emitting a broken one', () => {
    expect(mapBybitClosedPnl(row({ symbol: '' }), 'acct')).toBeNull();
    expect(mapBybitClosedPnl(row({ createdTime: '0' }), 'acct')).toBeNull();
  });
});
