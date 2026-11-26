import type { Position } from '../../generated/Position';

import { PositionSide } from '../../generated/PositionSide';
import { createPositionId } from '../../utils/createPositionId';

/** The generated contract represents an unavailable liquidation price as null. */
export function nativeLiquidationPrice(value: unknown): Position['liquidationPrice'] {
  const price = typeof value === 'number' ? value : typeof value === 'string' ? Number(value) : NaN;
  return Number.isFinite(price) && price > 0 ? price : null;
}

/** Explicit synthetic facts for tests that exercise only part of a position. */
export function nativePosition(row: Pick<Position, 'symbol'> & Partial<Position>): Position {
  const side = row.side ?? PositionSide.Long;
  return {
    id: createPositionId(row.symbol, side, row.positionMode),
    side,
    entryPrice: 0,
    notional: null,
    leverage: 1,
    unrealizedPnl: 0,
    contracts: 0,
    liquidationPrice: null,
    ...row,
  };
}
