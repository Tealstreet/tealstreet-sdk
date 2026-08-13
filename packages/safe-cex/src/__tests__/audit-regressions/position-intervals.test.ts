import type { Execution } from '../../types';

import { describe, expect, it } from 'vitest';

import { attributeFunding, reconstructPositionIntervals } from '../../journal/position-intervals';
import { OrderSide, PositionSide } from '../../types';

/** `Execution.timestamp` is SECONDS by contract — the fixtures honour that. */
const fill = (over: Partial<Execution> & { qty: number; side: OrderSide; timestamp: number }): Execution => ({
  symbol: 'DOGEUSDT',
  price: 0.07,
  ...over,
});

describe('reconstructPositionIntervals', () => {
  it('derives an open time for a simple round trip', () => {
    const intervals = reconstructPositionIntervals([
      fill({ qty: 100, side: OrderSide.Buy, timestamp: 1_000 }),
      fill({ qty: 100, side: OrderSide.Sell, timestamp: 1_600 }),
    ]);

    expect(intervals).toHaveLength(1);
    // Milliseconds out, seconds in.
    expect(intervals[0].openedAt).toBe(1_000_000);
    expect(intervals[0].closedAt).toBe(1_600_000);
    expect(intervals[0].side).toBe(PositionSide.Long);
    expect(intervals[0].complete).toBe(true);
  });

  it('keeps ONE interval across a partial-close ladder', () => {
    // The measured bybit ladder: open 3x, close in thirds. Bybit emits three closed rows
    // for this; there is exactly one position, and its open time is the FIRST fill.
    const intervals = reconstructPositionIntervals([
      fill({ qty: 141, side: OrderSide.Buy, timestamp: 100 }),
      fill({ qty: 141, side: OrderSide.Buy, timestamp: 110 }),
      fill({ qty: 141, side: OrderSide.Buy, timestamp: 120 }),
      fill({ qty: 141, side: OrderSide.Sell, timestamp: 200 }),
      fill({ qty: 141, side: OrderSide.Sell, timestamp: 210 }),
      fill({ qty: 141, side: OrderSide.Sell, timestamp: 220 }),
    ]);

    expect(intervals).toHaveLength(1);
    expect(intervals[0].openedAt).toBe(100_000);
    expect(intervals[0].closedAt).toBe(220_000);
    expect(intervals[0].maxSize).toBe(423);
    expect(intervals[0].closedSize).toBe(423);
  });

  it('splits a flip through zero into a close and an open at the same instant', () => {
    // Measured live: a sell of 283 against a 141 long leaves 142 SHORT. The venue reports
    // ONE fill with ONE realized PnL and no marker for where the close ends — it is
    // derivable only from the size held before the fill.
    const intervals = reconstructPositionIntervals([
      fill({ qty: 141, side: OrderSide.Buy, timestamp: 100 }),
      fill({ qty: 283, side: OrderSide.Sell, timestamp: 200 }),
    ]);

    expect(intervals).toHaveLength(2);
    const [closed, open] = intervals;

    expect(closed.side).toBe(PositionSide.Long);
    expect(closed.closedAt).toBe(200_000);
    expect(closed.closedByFlip).toBe(true);
    // Only the 141 that actually closed, NOT the 283 that was sold.
    expect(closed.closedSize).toBe(141);

    expect(open.side).toBe(PositionSide.Short);
    expect(open.openedAt).toBe(200_000);
    expect(open.closedAt).toBeUndefined();
    expect(open.maxSize).toBe(142);
  });

  it('tracks hedge legs as independent positions', () => {
    // A sell can OPEN the short leg or CLOSE the long one — measured 18 seconds apart on
    // okx. Without the leg they collapse into one bogus position.
    const intervals = reconstructPositionIntervals([
      fill({ qty: 100, side: OrderSide.Buy, timestamp: 100, positionSide: PositionSide.Long }),
      fill({ qty: 100, side: OrderSide.Sell, timestamp: 110, positionSide: PositionSide.Short }),
      fill({ qty: 100, side: OrderSide.Sell, timestamp: 200, positionSide: PositionSide.Long }),
      fill({ qty: 100, side: OrderSide.Buy, timestamp: 210, positionSide: PositionSide.Short }),
    ]);

    expect(intervals).toHaveLength(2);
    expect(intervals.map((i) => i.side).sort()).toEqual([PositionSide.Long, PositionSide.Short]);
    for (const interval of intervals) expect(interval.closedSize).toBe(100);
  });

  it('joins the leg from the order axis when fills do not carry one', () => {
    // Bybit puts the leg on NEITHER the fill nor the closed-PnL row — only on
    // /v5/order/history.positionIdx — so it arrives as a join on orderId.
    const legs: Record<string, PositionSide> = { o1: PositionSide.Long, o2: PositionSide.Short };
    const intervals = reconstructPositionIntervals(
      [
        fill({ qty: 50, side: OrderSide.Buy, timestamp: 100, orderId: 'o1' }),
        fill({ qty: 50, side: OrderSide.Sell, timestamp: 110, orderId: 'o2' }),
        fill({ qty: 50, side: OrderSide.Sell, timestamp: 200, orderId: 'o1' }),
      ],
      { resolveLeg: (e) => legs[e.orderId ?? ''] },
    );

    const long = intervals.find((i) => i.positionSide === PositionSide.Long);
    expect(long?.closedAt).toBe(200_000);
    expect(intervals.find((i) => i.positionSide === PositionSide.Short)?.closedAt).toBeUndefined();
  });

  it('marks a position already open at the window start as incomplete', () => {
    // The boundary case that makes funding attribution unsafe: the true open time is older
    // than any fill in view, so reporting the first visible fill would understate the hold
    // time AND miss the funding accrued before it.
    const intervals = reconstructPositionIntervals([fill({ qty: 100, side: OrderSide.Sell, timestamp: 500 })], {
      openingSize: { 'DOGEUSDT|both': 100 },
    });

    expect(intervals).toHaveLength(1);
    expect(intervals[0].complete).toBe(false);
    expect(intervals[0].closedAt).toBe(500_000);
  });

  it('emits a still-open position without a close time rather than dropping it', () => {
    const intervals = reconstructPositionIntervals([fill({ qty: 100, side: OrderSide.Buy, timestamp: 100 })]);
    expect(intervals).toHaveLength(1);
    expect(intervals[0].closedAt).toBeUndefined();
  });

  it('does not leave a residual position on float dust', () => {
    // Sizes are parsed decimals; 0.1+0.2-0.3 is not 0. An `=== 0` check would leave a
    // phantom position open forever and corrupt every later interval on the symbol.
    const intervals = reconstructPositionIntervals([
      fill({ qty: 0.1, side: OrderSide.Buy, timestamp: 100 }),
      fill({ qty: 0.2, side: OrderSide.Buy, timestamp: 110 }),
      fill({ qty: 0.3, side: OrderSide.Sell, timestamp: 200 }),
    ]);

    expect(intervals).toHaveLength(1);
    expect(intervals[0].closedAt).toBe(200_000);
  });

  it('separates two round trips on the same symbol', () => {
    const intervals = reconstructPositionIntervals([
      fill({ qty: 10, side: OrderSide.Buy, timestamp: 100 }),
      fill({ qty: 10, side: OrderSide.Sell, timestamp: 200 }),
      fill({ qty: 10, side: OrderSide.Buy, timestamp: 300 }),
      fill({ qty: 10, side: OrderSide.Sell, timestamp: 400 }),
    ]);

    expect(intervals).toHaveLength(2);
    expect(intervals[0].openedAt).toBe(100_000);
    expect(intervals[1].openedAt).toBe(300_000);
  });

  it('derives a size-weighted entry and exit price', () => {
    // `deriveBinanceClosedPositions` sets entryPrice: 0 because a single closing fill cannot
    // know it. Running state can, and binance has no closed-position endpoint at all — so
    // this is the ONLY source of an entry price on that venue.
    const intervals = reconstructPositionIntervals([
      fill({ qty: 100, side: OrderSide.Buy, timestamp: 100, price: 10 }),
      fill({ qty: 300, side: OrderSide.Buy, timestamp: 110, price: 20 }),
      fill({ qty: 400, side: OrderSide.Sell, timestamp: 200, price: 30 }),
    ]);

    // (100*10 + 300*20) / 400 = 17.5 — weighted by size, not a mean of prices (15).
    expect(intervals[0].avgEntryPrice).toBeCloseTo(17.5, 10);
    expect(intervals[0].avgExitPrice).toBeCloseTo(30, 10);
  });

  it('sums venue-reported PnL and fees, and distinguishes unreported from zero', () => {
    const withPnl = reconstructPositionIntervals([
      fill({ qty: 10, side: OrderSide.Buy, timestamp: 100, commission: 0.01, commissionAsset: 'USDT' }),
      fill({
        qty: 10,
        side: OrderSide.Sell,
        timestamp: 200,
        realizedPnl: 1.5,
        commission: 0.02,
        commissionAsset: 'USDT',
      }),
    ]);
    expect(withPnl[0].realizedPnl).toBeCloseTo(1.5, 10);
    // Fees accrue on EVERY fill, opening included — the opening commission is real money.
    expect(withPnl[0].fees).toBeCloseTo(0.03, 10);
    expect(withPnl[0].feeCurrencies).toEqual(['USDT']);

    // A venue that reports no PnL on fills must stay undefined, not 0 — otherwise
    // "the venue said zero" and "the venue said nothing" become indistinguishable.
    const withoutPnl = reconstructPositionIntervals([
      fill({ qty: 10, side: OrderSide.Buy, timestamp: 100 }),
      fill({ qty: 10, side: OrderSide.Sell, timestamp: 200 }),
    ]);
    expect(withoutPnl[0].realizedPnl).toBeUndefined();
  });

  it('records every fee currency, because binance can charge in BNB', () => {
    // A single summed `fees` number is only meaningful in one currency. Binance pays
    // commission in BNB when the account opts in, while the position settles in USDT —
    // adding those together is adding different units.
    const intervals = reconstructPositionIntervals([
      fill({ qty: 10, side: OrderSide.Buy, timestamp: 100, commission: 0.01, commissionAsset: 'BNB' }),
      fill({ qty: 10, side: OrderSide.Sell, timestamp: 200, commission: 0.02, commissionAsset: 'USDT' }),
    ]);
    expect(intervals[0].feeCurrencies.sort()).toEqual(['BNB', 'USDT']);
  });

  it('splits the fee on a flip but not the realized PnL', () => {
    // One fill, one commission, but only part of it closed the old position. The fee is
    // charged on the whole fill so it splits by size; realized PnL is NOT split — the
    // opening portion cannot have realized anything.
    const intervals = reconstructPositionIntervals([
      fill({ qty: 100, side: OrderSide.Buy, timestamp: 100, price: 10 }),
      fill({
        qty: 300,
        side: OrderSide.Sell,
        timestamp: 200,
        price: 12,
        realizedPnl: 200,
        commission: 0.3,
        commissionAsset: 'USDT',
      }),
    ]);

    const [closed, opened] = intervals;
    // 100 of the 300 closed => one third of the fee.
    expect(closed.fees).toBeCloseTo(0.1, 10);
    expect(closed.realizedPnl).toBeCloseTo(200, 10);
    // The remaining 200 opened the short, carrying the other two thirds.
    expect(opened.fees).toBeCloseTo(0.2, 10);
    expect(opened.realizedPnl).toBeUndefined();
    expect(opened.avgEntryPrice).toBeCloseTo(12, 10);
  });

  it('sorts unordered fills before folding', () => {
    // Venue responses are newest-first as often as not; folding them in that order
    // produces a running size that is nonsense.
    const intervals = reconstructPositionIntervals([
      fill({ qty: 100, side: OrderSide.Sell, timestamp: 200 }),
      fill({ qty: 100, side: OrderSide.Buy, timestamp: 100 }),
    ]);

    expect(intervals).toHaveLength(1);
    expect(intervals[0].side).toBe(PositionSide.Long);
    expect(intervals[0].openedAt).toBe(100_000);
  });
});

describe('attributeFunding', () => {
  const interval = {
    symbol: 'DOGEUSDT',
    positionSide: 'both' as const,
    side: PositionSide.Long,
    openedAt: 1_000,
    closedAt: 5_000,
    maxSize: 100,
    closedSize: 100,
    complete: true,
    closedByFlip: false,
    avgEntryPrice: 0.07,
    avgExitPrice: 0.07,
    fees: 0,
    feeCurrencies: [],
  };

  const rows = [
    { symbol: 'DOGEUSDT', timestamp: 500, income: -1 }, // before the position opened
    { symbol: 'DOGEUSDT', timestamp: 2_000, income: -3 },
    { symbol: 'DOGEUSDT', timestamp: 4_000, income: 2 },
    { symbol: 'DOGEUSDT', timestamp: 6_000, income: -7 }, // after it closed
    { symbol: 'BTCUSDT', timestamp: 3_000, income: -100 }, // another symbol
  ];

  it('sums only funding inside the interval, for this symbol', () => {
    expect(attributeFunding(interval, rows, { hedged: false })).toEqual({ funding: -1, attributed: true });
  });

  it('reports the symbol total but refuses to attribute it in hedge mode', () => {
    // Funding arrives per SYMBOL, not per leg. With both legs open there is no
    // venue-reported split, and inventing one is a fabrication.
    expect(attributeFunding(interval, rows, { hedged: true })).toEqual({ funding: -1, attributed: false });
  });

  it('refuses to attribute when the interval started before the window', () => {
    // Funding accrued before the first visible fill is invisible, so the sum is a floor,
    // not the answer — and a floor presented as exact is how a wrong number survives.
    const incomplete = { ...interval, complete: false };
    expect(attributeFunding(incomplete, rows, { hedged: false })).toEqual({ funding: 0, attributed: false });
  });

  it('refuses to attribute to a position that is still open', () => {
    const open = { ...interval, closedAt: undefined };
    expect(attributeFunding(open, rows, { hedged: false })).toEqual({ funding: 0, attributed: false });
  });
});
