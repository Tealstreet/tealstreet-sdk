import { Position, PositionSide } from '../../types';

// this version ignores upnl and calculates raw % based on exit price
export function calculateRelativeReturns(position: Position, exitPrice: number) {
  const upnl = position.side === PositionSide.Long ? exitPrice - position.entryPrice : position.entryPrice - exitPrice;
  const margin = position.entryPrice;
  if (!margin) {
    return { roe: 0, roi: 0 };
  }
  const roe = (upnl / margin) * position.leverage * 100;
  const roi = (upnl / margin) * 100;
  return { roe, roi };
}
