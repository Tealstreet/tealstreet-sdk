import { describe, expect, it } from 'vitest';

import { deriveBinanceClosedPositions } from '../reference/binance/binance.closed-pnl';
import { PositionSide } from '../../types';

/** Row shape captured live from `/fapi/v1/userTrades` (sheet row 2). */
const trade = (over: Record<string, any> = {}) => ({
  symbol: 'DOGEUSDT',
  id: 3422500270,
  orderId: 101205954323,
  side: 'SELL',
  price: '0.069980',
  qty: '143',
  realizedPnl: '0.12',
  quoteQty: '10.007140',
  commission: '0.00500357',
  commissionAsset: 'USDT',
  time: 1786158465224,
  positionSide: 'LONG',
  maker: false,
  ...over,
});

describe('binance derived closed positions', () => {
  it('skips opening fills, which have no realized PnL', () => {
    // A fill with realizedPnl 0 closed nothing; emitting it would inflate trade counts
    // and add rows that are not closed positions.
    expect(deriveBinanceClosedPositions([trade({ realizedPnl: '0' })], 'acct')).toHaveLength(0);
  });

  it('emits a closed position for a fill that realized PnL', () => {
    const entries = deriveBinanceClosedPositions([trade()], 'acct');

    expect(entries).toHaveLength(1);
    expect(entries[0].unrealizedPnl).toBeCloseTo(0.12, 12);
  });

  it('is stamped derived, because it is a fold rather than a venue report', () => {
    expect(deriveBinanceClosedPositions([trade()], 'acct')[0].provenance).toBe('derived');
  });

  it('inverts the side: a closing SELL means the position was long', () => {
    expect(deriveBinanceClosedPositions([trade({ side: 'SELL' })], 'acct')[0].side).toBe(PositionSide.Long);
    expect(deriveBinanceClosedPositions([trade({ side: 'BUY' })], 'acct')[0].side).toBe(PositionSide.Short);
  });

  it('keeps realizedPnl GROSS of the commission on the same row', () => {
    // Third convention across the venues: bybit's closedPnl is net of fees, okx's is net
    // of fees AND funding, binance's is neither. Anything comparing against a ledger has
    // to know which, or the residual is exactly the size of the term it got wrong.
    const entry = deriveBinanceClosedPositions([trade()], 'acct')[0];

    expect(entry.unrealizedPnl).toBeCloseTo(0.12, 12);
    expect(entry.fees).toBeCloseTo(0.00500357, 12);
  });

  it('leaves entryPrice at 0 rather than approximating it', () => {
    // A fill knows its own price, not the average entry of the position it closes. A
    // plausible wrong entry silently corrupts every return percentage downstream.
    expect(deriveBinanceClosedPositions([trade()], 'acct')[0].entryPrice).toBe(0);
  });

  it('drops rows with no symbol or timestamp instead of emitting broken ones', () => {
    expect(deriveBinanceClosedPositions([trade({ symbol: '' }), trade({ time: 0 })], 'acct')).toHaveLength(0);
  });
});
