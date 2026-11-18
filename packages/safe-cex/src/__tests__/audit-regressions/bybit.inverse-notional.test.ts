import type { Position } from '../../types';

import { describe, expect, it } from 'vitest';

import { PositionSide } from '../../types';
import { positionUpnlInQuote } from '../reference/bybit/bybit.inverse';

const position = (notional: number | null, symbol = 'BTCUSD'): Position => ({
  id: 'p',
  symbol,
  side: PositionSide.Long,
  entryPrice: 100,
  notional,
  leverage: 5,
  unrealizedPnl: 5,
  contracts: 2,
  liquidationPrice: null,
});

describe('inverse quote PnL with nullable position valuation', () => {
  it.each([null, NaN, Infinity, -Infinity])('preserves serialized %s as unknown', (notional) => {
    const row: Position = JSON.parse(JSON.stringify(position(notional)));
    expect(positionUpnlInQuote(row)).toBeNaN();
  });

  it.each([0, 250])('preserves known notional %s', (notional) => {
    expect(positionUpnlInQuote(position(notional))).toBe((5 * notional) / 2);
  });

  it('keeps independently reported linear PnL usable', () => {
    expect(positionUpnlInQuote(position(null, 'BTCUSDT'))).toBe(5);
  });
});
