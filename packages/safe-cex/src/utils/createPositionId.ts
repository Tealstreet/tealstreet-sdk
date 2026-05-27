import { PositionMode, PositionSide } from '../types';

export const createPositionId = (symbol: string, side?: PositionSide, mode?: PositionMode) => {
  return mode === PositionMode.OneWay || !side ? symbol : `${symbol}:${side}`;
};
