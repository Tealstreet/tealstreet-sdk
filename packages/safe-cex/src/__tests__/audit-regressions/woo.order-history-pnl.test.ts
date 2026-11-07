import { describe, expect, it } from 'vitest';

import { mapWooHistoricalOrder } from '../reference/woo/woo.order-history';

/**
 * WOO's `/v1/orders` PnL fields, and the presence-vs-zero distinction they turn on.
 *
 * This venue was recorded UNSUPPORTABLE for a week on the strength of a real measurement —
 * `realizedPnl` is `null` on 534/534 rows of `/v3/trade/transactionHistory`. That number was
 * correct about the one route the adapter read, and wrong about the venue: `/v1/orders`
 * carries per-order `realized_pnl`, `total_fee`, `total_rebate` and `position_side`, but
 * only when the request asks for them.
 *
 * Rows below are transcribed from a live probe of sheet row 28.
 */

const resolveSymbol = (marketId: string) => (marketId === 'PERP_ETH_USDT' ? 'ETHUSDT' : undefined);

const liveRow = {
  order_id: 82471810446,
  symbol: 'PERP_ETH_USDT',
  side: 'BUY',
  type: 'IOC',
  status: 'FILLED',
  price: 1913.1,
  quantity: 0.0106,
  executed: 0.0106,
  average_executed_price: 1899.3,
  reduce_only: true,
  position_side: 'SHORT',
  created_time: '1786603865.910',
  updated_time: '1786603865.910',
  total_fee: 0.00181194,
  fee_asset: 'USDT',
  total_rebate: 0,
  rebate_asset: 'USDT',
  realized_pnl: -0.11236,
};

describe('mapWooHistoricalOrder — PnL fields', () => {
  it('carries realized PnL, fee and rebate off a live row', () => {
    const mapped = mapWooHistoricalOrder(liveRow, resolveSymbol);

    expect(mapped).toMatchObject({
      symbol: 'ETHUSDT',
      realizedPnl: -0.11236,
      fees: 0.00181194,
      feeAsset: 'USDT',
      rebate: 0,
      rebateAsset: 'USDT',
    });
  });

  it('keeps fees POSITIVE for a cost, matching PositionRecord.fees', () => {
    // The paired ledger row reads -0.00181194; the order route reports the same charge
    // positive. Negating here would double-flip it against the canonical convention.
    expect(mapWooHistoricalOrder(liveRow, resolveSymbol)?.fees).toBeGreaterThan(0);
  });

  it('distinguishes an ABSENT field from a zero one', () => {
    // The fields simply do not appear unless `realized_pnl=true` was sent. A breakeven
    // close legitimately realizes 0, so folding 0 into undefined would recreate exactly the
    // ambiguity that got this venue written off.
    const withoutFlag = { ...liveRow } as Record<string, any>;
    delete withoutFlag.realized_pnl;
    delete withoutFlag.total_fee;

    const mapped = mapWooHistoricalOrder(withoutFlag, resolveSymbol);
    expect(mapped?.realizedPnl).toBeUndefined();
    expect(mapped?.fees).toBeUndefined();

    const breakeven = mapWooHistoricalOrder({ ...liveRow, realized_pnl: 0 }, resolveSymbol);
    expect(breakeven?.realizedPnl).toBe(0);
  });

  it('treats an explicit null as absent, not as zero', () => {
    // `Number(null)` is 0 — the precise coercion that turned "this route carries no PnL"
    // into "this account made no PnL" on the v3 fills route.
    const mapped = mapWooHistoricalOrder({ ...liveRow, realized_pnl: null }, resolveSymbol);
    expect(mapped?.realizedPnl).toBeUndefined();
  });

  it('reads the hedge leg from position_side, never from side', () => {
    // The live row is a BUY that closed a SHORT. Deriving the leg from `side` inverts it.
    expect(mapWooHistoricalOrder(liveRow, resolveSymbol)?.positionSide).toBe('short');
  });

  it('scales fractional-second timestamps to milliseconds', () => {
    expect(mapWooHistoricalOrder(liveRow, resolveSymbol)?.createdTime).toBe(1786603865910);
  });
});
