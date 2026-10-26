import type { Balance, Position } from '../../../types';

import { positionUpnlInQuote } from '../../../exchanges/bybit/bybit.inverse';

export const syncUnifiedBalanceUpnlFromPositions = (
  balance: Balance | undefined,
  positions: Position[],
  accountType: 'UNIFIED' | 'CONTRACT',
): Balance | undefined => {
  if (!balance || accountType !== 'UNIFIED') {
    return balance;
  }

  const upnl = positions.reduce((sum, position) => sum + positionUpnlInQuote(position), 0);

  if (balance.upnl === upnl) {
    return balance;
  }

  return {
    ...balance,
    upnl,
  };
};
