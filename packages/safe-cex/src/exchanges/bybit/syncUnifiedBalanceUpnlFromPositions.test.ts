import { describe, expect, it } from 'vitest';

import { PositionSide } from '../../types';
import { syncUnifiedBalanceUpnlFromPositions } from './syncUnifiedBalanceUpnlFromPositions';

describe('syncUnifiedBalanceUpnlFromPositions', () => {
  it('keeps contract balances unchanged', () => {
    const balance = { total: 1000, free: 900, used: 100, upnl: 25 };

    const result = syncUnifiedBalanceUpnlFromPositions(balance, [], 'CONTRACT');

    expect(result).toBe(balance);
  });

  it('updates unified balance upnl from current positions without changing wallet balance', () => {
    const balance = {
      total: 3200,
      free: 2500,
      used: 700,
      upnl: 100,
      info: { totalEquity: '3300', accountType: 'UNIFIED' },
    };

    const positions = [
      {
        id: 'long-rave',
        symbol: 'RAVEUSDT',
        side: PositionSide.Long,
        entryPrice: 10,
        notional: 1000,
        leverage: 10,
        unrealizedPnl: 450,
        realizedPnl: 0,
        contracts: 100,
        liquidationPrice: 0,
      },
      {
        id: 'short-btc',
        symbol: 'BTCUSDT',
        side: PositionSide.Short,
        entryPrice: 70000,
        notional: 5000,
        leverage: 5,
        unrealizedPnl: -25,
        realizedPnl: 0,
        contracts: 0.1,
        liquidationPrice: 0,
      },
    ];

    const result = syncUnifiedBalanceUpnlFromPositions(balance, positions, 'UNIFIED');

    expect(result).toEqual({
      ...balance,
      upnl: 425,
    });
    expect(result?.total).toBe(3200);
  });
});
