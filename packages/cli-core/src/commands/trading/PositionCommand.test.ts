import { expect, it, vi } from 'vitest';

import { PositionSide, type Market, type Position } from '@tealstreet/safe-cex/src/types';
import type { CLIPlatformAdapter, ExchangeLike } from '../../types';
import { PositionCommand } from './PositionCommand';

it('p shows an OKX DOGE position in base units', async () => {
  const market = { symbol: 'DOGEUSDT', base: 'DOGE', precision: { amount: 0.01, price: 0.00001 }, contractSize: 1000 };
  const position = {
    symbol: 'DOGEUSDT',
    side: PositionSide.Long,
    contracts: 240,
    entryPrice: 0.1,
    notional: 24,
    unrealizedPnl: 0,
  };
  const store = { markets: [market as Market], positions: [position as Position], orders: [] };
  const print = vi.fn();
  const adapter = { print } as unknown as CLIPlatformAdapter;
  const exchange = { store } as ExchangeLike;
  await new PositionCommand().execute({ raw: 'p', args: { _: ['p'] }, symbol: 'DOGEUSDT', exchange, adapter });

  expect(print.mock.calls.flat(2).join('\n')).toContain('Size:   240 (');
});
