import { Position } from '../../types';

export function calculateReturns(
  position: Pick<Position, 'unrealizedPnl' | 'contracts' | 'entryPrice' | 'leverage'>,
  overrideUPNL?: number,
) {
  const upnl = overrideUPNL ? overrideUPNL : position.unrealizedPnl;
  const margin = position.entryPrice * position.contracts;
  if (!margin) {
    return { roe: 0, roi: 0 };
  }
  const roe = (upnl / margin) * position.leverage * 100;
  const roi = (upnl / margin) * 100;
  return { roe, roi };
}
