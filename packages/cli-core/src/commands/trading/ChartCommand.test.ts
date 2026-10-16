import { expect, it } from 'vitest';

import { PositionSide, type Market, type Position } from '@tealstreet/safe-cex/src/types';
import { formatPositionLine, formatPositionSummary } from './ChartCommand';

it('chart annotations preserve positions smaller than four decimal places', () => {
  const market = { base: 'BTC', precision: { amount: 0.000001, price: 0.1 }, contractSize: 1 } as Market;
  const position = { side: PositionSide.Long, contracts: 0.000006, entryPrice: 60_000, unrealizedPnl: 0 } as Position;

  expect(formatPositionLine(position, market)?.label).toContain('Long 0.000006');
  expect(formatPositionSummary(position, market)?.replace(/\x1b\[[0-9;]*m/g, '')).toContain('Long 0.000006');
});
