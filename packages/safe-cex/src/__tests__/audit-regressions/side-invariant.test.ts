import type { PositionRecord } from '../../journal/position.types';

import { describe, expect, it } from 'vitest';

import { checkFeeConvention, checkPnlMagnitude, checkSideConsistency } from '../../journal/side-invariant';
import { PositionSide } from '../../types';

const record = (over: Partial<PositionRecord> = {}): PositionRecord => ({
  key: 'k',
  account: 'a',
  symbol: 'LINKUSDT',
  settleCurrency: 'USDT',
  side: PositionSide.Long,
  positionSide: 'both',
  state: 'closed',
  closedAt: 2,
  closedSize: 10,
  avgEntryPrice: 100,
  avgExitPrice: 110,
  realizedPnl: 100,
  fees: 0.1,
  funding: 0,
  fundingAttributed: true,
  provenance: 'reported',
  sourceAxis: 'closed-positions',
  closeCount: 1,
  ...over,
});

describe('checkSideConsistency', () => {
  it('accepts a long that profited as the price rose', () => {
    const r = checkSideConsistency([record()]);
    expect(r).toMatchObject({ consistent: 1, unchecked: 0 });
    expect(r.contradictions).toHaveLength(0);
  });

  it('accepts a short that profited as the price fell', () => {
    const r = checkSideConsistency([
      record({ side: PositionSide.Short, avgEntryPrice: 110, avgExitPrice: 100, realizedPnl: 100 }),
    ]);
    expect(r.consistent).toBe(1);
    expect(r.contradictions).toHaveLength(0);
  });

  it('CATCHES an inverted side — the failure no identity check can see', () => {
    // This is the whole point. Reading a venue's BUY/SELL as an order side rather than the
    // position's direction inverts every record, and every total still reconciles because
    // the sum is unchanged. Only the per-record price correlation exposes it.
    const r = checkSideConsistency([
      record({ side: PositionSide.Short, avgEntryPrice: 100, avgExitPrice: 110, realizedPnl: 100 }),
    ]);
    expect(r.consistent).toBe(0);
    expect(r.contradictions).toHaveLength(1);
    expect(r.contradictions[0]).toContain('LINKUSDT');
  });

  it('reports entry == exit as UNCHECKED, never as consistent', () => {
    // Measured on bitunix: a flat round trip whose gross is exactly zero. It carries no
    // directional signal, and counting it as a pass would let a venue with only flat trades
    // look fully verified.
    const r = checkSideConsistency([record({ avgEntryPrice: 8.767, avgExitPrice: 8.767, realizedPnl: 0 })]);
    expect(r.unchecked).toBe(1);
    expect(r.consistent).toBe(0);
  });

  it('reports a move below the noise floor as UNCHECKED', () => {
    // Expected PnL is a rounding artefact at this scale, so its sign is not evidence.
    const r = checkSideConsistency([
      record({ avgEntryPrice: 100, avgExitPrice: 100.0000000001, closedSize: 0.0001, realizedPnl: 1e-12 }),
    ]);
    expect(r.unchecked).toBe(1);
  });

  it('reports a funding-dominated record as UNCHECKED', () => {
    // Where funding could not be separated out it sits inside realizedPnl and can flip its
    // sign on a long-held, low-PnL position — so the correlation is meaningless there.
    const r = checkSideConsistency([record({ realizedPnl: -0.5, funding: -2 })]);
    expect(r.unchecked).toBe(1);
    expect(r.contradictions).toHaveLength(0);
  });

  it('reports a missing exit price as UNCHECKED', () => {
    const r = checkSideConsistency([record({ avgExitPrice: undefined })]);
    expect(r.unchecked).toBe(1);
  });

  it('separates unchecked from consistent across a mixed set', () => {
    // The distinction the census must preserve: "82 consistent, 0 unchecked" and
    // "0 consistent, 82 unchecked" are wildly different evidence.
    const r = checkSideConsistency([
      record(),
      record({ avgEntryPrice: 50, avgExitPrice: 50, realizedPnl: 0 }),
      record({ side: PositionSide.Short, avgEntryPrice: 100, avgExitPrice: 110, realizedPnl: 100 }),
    ]);
    expect(r).toMatchObject({ consistent: 1, unchecked: 1 });
    expect(r.contradictions).toHaveLength(1);
  });
});

describe('checkPnlMagnitude', () => {
  it('accepts a PnL that matches the price move', () => {
    // 10 units from 100 to 110 is exactly 100 — not approximately.
    const r = checkPnlMagnitude([record()]);
    expect(r).toMatchObject({ consistent: 1, unchecked: 0 });
    expect(r.mismatches).toHaveLength(0);
  });

  it('CATCHES a 1e6 scale error — invisible to all three other invariants', () => {
    // BitMEX reports money in minor units: walletBalance 60827952 USDt on a ~$60 account.
    // A record built with the wrong power of ten reconciles against a ledger scaled the
    // same way, keeps its sign, and is non-zero — so only magnitude exposes it.
    const r = checkPnlMagnitude([record({ realizedPnl: 100_000_000 })]);
    expect(r.consistent).toBe(0);
    expect(r.mismatches).toHaveLength(1);
    expect(r.mismatches[0]).toContain('ratio=');
  });

  it('CATCHES a 1000x size error, the shape found on blofin', () => {
    // closedSize inflated by the market's contractSize while prices stayed correct.
    const r = checkPnlMagnitude([record({ closedSize: 10_000 })]);
    expect(r.mismatches).toHaveLength(1);
  });

  it('tolerates ordinary imprecision — it hunts orders of magnitude, not pennies', () => {
    // Nearer than a factor of two is the identity's job, not this one's.
    expect(checkPnlMagnitude([record({ realizedPnl: 130 })]).consistent).toBe(1);
  });

  it('reports the same UNCHECKED cases as the side invariant', () => {
    expect(checkPnlMagnitude([record({ avgEntryPrice: 100, avgExitPrice: 100 })]).unchecked).toBe(1);
    expect(checkPnlMagnitude([record({ avgExitPrice: undefined })]).unchecked).toBe(1);
    // Funding folded into the figure moves its magnitude, so it is not a clean comparison.
    expect(checkPnlMagnitude([record({ realizedPnl: 1, funding: 5 })]).unchecked).toBe(1);
  });
});

describe('checkFeeConvention', () => {
  it('accepts a fee that is a cost of a plausible size', () => {
    // 0.1 on a 1000 notional = 1bp. Ordinary.
    const r = checkFeeConvention([record({ fees: 0.1 })]);
    expect(r).toMatchObject({ consistent: 1, rebates: 0, unchecked: 0 });
    expect(r.implausible).toHaveLength(0);
  });

  it('CATCHES an inverted fee sign — invisible to all four other invariants', () => {
    // The whole point. blofin signs `fee` negative when charged, bitunix positive, bitget
    // negative. Negate the wrong one and every commission becomes a rebate — while the
    // gross-vs-gross identity, the side check, the magnitude check and the zero-refusal all
    // still pass, because none of them looks at fees.
    const inverted = [record({ fees: -0.1 }), record({ fees: -0.2 }), record({ fees: -0.15 })];
    const r = checkFeeConvention(inverted);
    expect(r.rebates).toBe(3);
    expect(r.consistent).toBe(0);
  });

  it('does not flag a SINGLE rebate as a defect', () => {
    // Maker rebates are real. The signal is the rate, not any one record, which is why the
    // count is reported rather than a per-record failure.
    const r = checkFeeConvention([record({ fees: 0.1 }), record({ fees: 0.1 }), record({ fees: -0.05 })]);
    expect(r).toMatchObject({ consistent: 2, rebates: 1 });
  });

  it('CATCHES a fee too large to be a trading fee', () => {
    // 50 on a 1000 notional is 5%. Not a fee — a units error, a wrong field, or a scale bug.
    const r = checkFeeConvention([record({ fees: 50 })]);
    expect(r.implausible).toHaveLength(1);
    expect(r.implausible[0]).toContain('% of notional');
    expect(r.consistent).toBe(0);
  });

  it('reports a zero fee as UNCHECKED, never as consistent', () => {
    // A venue that simply does not report fees must not read as "fees verified".
    expect(checkFeeConvention([record({ fees: 0 })])).toMatchObject({ unchecked: 1, consistent: 0 });
  });

  it('reports a missing notional as UNCHECKED', () => {
    expect(checkFeeConvention([record({ fees: 0.1, avgEntryPrice: 0 })])).toMatchObject({ unchecked: 1 });
    expect(checkFeeConvention([record({ fees: 0.1, closedSize: 0 })])).toMatchObject({ unchecked: 1 });
  });
});
