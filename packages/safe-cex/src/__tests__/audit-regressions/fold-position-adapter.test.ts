import type { PositionInterval } from '../../journal/position-intervals';

import { describe, expect, it } from 'vitest';

import { mapFoldedInterval } from '../../journal/fold.position-adapter';
import { reconstructPositionIntervals } from '../../journal/position-intervals';
import { OrderSide, PositionSide } from '../../types';

const deps = { account: 'test', resolveSettleCurrency: () => 'USDT' };

const interval = (over: Partial<PositionInterval> = {}): PositionInterval => ({
  symbol: 'BTCUSDT',
  positionSide: 'both',
  side: PositionSide.Long,
  openedAt: 1_000_000,
  closedAt: 2_000_000,
  maxSize: 1,
  closedSize: 1,
  complete: true,
  closedByFlip: false,
  avgEntryPrice: 100,
  avgExitPrice: 110,
  realizedPnl: 10,
  fees: 0.04,
  feeCurrencies: ['USDT'],
  ...over,
});

describe('folded-interval position adapter (binance / hyperliquid)', () => {
  it('passes realizedPnl through GROSS, without adding fees back', () => {
    // The third convention. Binance reports realizedPnl gross with `commission` as a
    // separate field, so the bybit/okx treatment of adding fees back would double-count
    // the fee — and would still reconcile in aggregate, which is why it needs a test.
    const record = mapFoldedInterval(interval(), deps)!;
    expect(record.realizedPnl).toBeCloseTo(10, 10);
    expect(record.fees).toBeCloseTo(0.04, 10);
  });

  it('marks every record derived, because binance has no closed-position endpoint', () => {
    // Path-dependent: one missing fill corrupts every later position on the symbol, unlike
    // a venue-reported row whose error is local. Consumers must not present these as
    // verified.
    const record = mapFoldedInterval(interval(), deps)!;
    expect(record.provenance).toBe('derived');
    expect(record.sourceAxis).toBe('fills');
  });

  it('carries a real open time and a derived entry price', () => {
    // `deriveBinanceClosedPositions` reports entryPrice: 0 because a closing fill cannot
    // know it. The fold can.
    const record = mapFoldedInterval(interval(), deps)!;
    expect(record.openedAt).toBe(1_000_000);
    expect(record.avgEntryPrice).toBeCloseTo(100, 10);
  });

  it('drops the open time when the position predates the fills in view', () => {
    const record = mapFoldedInterval(interval({ complete: false }), deps)!;
    expect(record.openedAt).toBeUndefined();
  });

  it('leaves closeReason undefined rather than asserting a user close', () => {
    // A binance fill carries no close reason; a liquidation shows up as its own ledger
    // income type. Defaulting to 'user' would assert something the axis cannot know.
    expect(mapFoldedInterval(interval(), deps)!.closeReason).toBeUndefined();
  });

  it('REFUSES a record when the venue reported no PnL at all, rather than emitting zero', () => {
    // Measured on woo: realizedPnl is a field on its fills and is null on 534/534 rows.
    // With no ledger axis and no closed-position axis, a defaulted 0 would be an
    // unfalsifiable claim that every round trip broke even.
    expect(mapFoldedInterval(interval({ realizedPnl: undefined }), deps)).toBeNull();
    // A genuine zero from a venue that DOES report still passes — binance writes 0 on
    // opening fills, and "the venue said zero" is not "the venue said nothing".
    expect(mapFoldedInterval(interval({ realizedPnl: 0 }), deps)?.realizedPnl).toBe(0);
  });

  it('does not emit a record for a position that is still open', () => {
    expect(mapFoldedInterval(interval({ closedAt: undefined }), deps)).toBeNull();
  });

  it('keys hedge legs apart when they close at the same instant', () => {
    // A hedge flatten closes both legs together; a key without the leg deletes one.
    const long = mapFoldedInterval(interval({ positionSide: PositionSide.Long }), deps)!;
    const short = mapFoldedInterval(interval({ positionSide: PositionSide.Short }), deps)!;
    expect(long.key).not.toBe(short.key);
  });

  it('builds a whole round trip end-to-end from raw fills', () => {
    const intervals = reconstructPositionIntervals([
      {
        symbol: 'BTCUSDT',
        timestamp: 1_000,
        price: 100,
        qty: 2,
        side: OrderSide.Buy,
        commission: 0.02,
        commissionAsset: 'USDT',
      },
      {
        symbol: 'BTCUSDT',
        timestamp: 2_000,
        price: 110,
        qty: 2,
        side: OrderSide.Sell,
        realizedPnl: 20,
        commission: 0.022,
        commissionAsset: 'USDT',
      },
    ]);

    const record = mapFoldedInterval(intervals[0], deps)!;
    expect(record.side).toBe(PositionSide.Long);
    expect(record.openedAt).toBe(1_000_000);
    expect(record.closedAt).toBe(2_000_000);
    expect(record.avgEntryPrice).toBeCloseTo(100, 10);
    expect(record.avgExitPrice).toBeCloseTo(110, 10);
    expect(record.realizedPnl).toBeCloseTo(20, 10);
    // Both fills' commission, opening included.
    expect(record.fees).toBeCloseTo(0.042, 10);
  });
});
