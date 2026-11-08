import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { Market, Position } from '../types';
import type { Store } from './store.interface';

import { positionEnums } from '@tealstreet/safe-cex-wasm/wire';
import { MarginType, PositionMode, PositionSide } from '../types';

const sides = { long: PositionSide.Long, short: PositionSide.Short };
const modes = { hedged: PositionMode.Hedged, oneway: PositionMode.OneWay };
const margins = { cross: MarginType.Cross, isolated: MarginType.Isolated };

/** Decode the native Position contract without repairing numbers, sizes or identity. */
export function readPositionFrame(frame: Frame, store: Store, markets?: readonly Market[]): Position[] | undefined {
  const positions = frame.positions();
  if (positions) {
    const rows: Position[] = [];
    const greekTable = frame.positionGreeks?.();
    const greeksById = new Map<string, NonNullable<Position['option']>>();
    if (greekTable) {
      for (let i = 0; i < greekTable.count; i++) {
        greeksById.set(greekTable.positionId(i), {
          greeks: {
            delta: greekTable.delta[i],
            gamma: greekTable.gamma[i],
            vega: greekTable.vega[i],
            theta: greekTable.theta[i],
            units: greekTable.units(i),
          },
        });
      }
    }
    const predictionHoldings = frame.predictionHoldings?.();
    const holdingFlags = predictionHoldings && new Map<number, number>();
    if (predictionHoldings && holdingFlags) {
      for (let i = 0; i < predictionHoldings.count; i++)
        holdingFlags.set(predictionHoldings.market[i], predictionHoldings.flags[i]);
    }
    for (let i = 0; i < positions.count; i++) {
      const enums = positionEnums(positions.enums[i]);
      const symbol = positions.symbol(i);
      const market = markets ? markets[positions.market[i]] : store.marketsBySymbol[symbol];
      const mode = enums.positionMode === undefined ? undefined : modes[enums.positionMode];
      if (enums.side === undefined) throw new Error('missing Position side');
      const id = positions.id(i);
      const contractsNative = positions.contractsNative[i];
      const unrealizedPnlNative = positions.unrealizedPnlNative[i];
      // Native null means unknown. Preserve it without a numeric fallback.
      const liquidationPrice = positions.liquidationPrice[i];
      const realizedPnl = positions.realizedPnl[i];
      const marginSize = positions.marginSize[i];
      const lastUpdate = positions.lastUpdate[i];
      rows.push({
        id,
        ...(greeksById.has(id) && { option: greeksById.get(id) }),
        symbol,
        side: sides[enums.side],
        ...(market?.info?.conditionId && {
          info: {
            tokenId: market.id,
            conditionId: market.info.conditionId,
            redeemable: ((holdingFlags?.get(positions.market[i]) ?? 0) & 1) !== 0,
            mergeable: ((holdingFlags?.get(positions.market[i]) ?? 0) & 2) !== 0,
          },
        }),
        entryPrice: positions.entryPrice[i],
        notional: positions.notional[i],
        leverage: positions.leverage[i],
        unrealizedPnl: positions.unrealizedPnl[i],
        contracts: positions.contracts[i],
        ...((contractsNative !== undefined || unrealizedPnlNative !== undefined) && {
          extra: {
            ...(contractsNative !== undefined && { contractsNative }),
            ...(unrealizedPnlNative !== undefined && { unrealizedPnlNative }),
          },
        }),
        liquidationPrice,
        ...(mode !== undefined && { positionMode: mode }),
        ...(enums.marginType !== undefined && { marginType: margins[enums.marginType] }),
        ...(realizedPnl !== undefined && { realizedPnl }),
        ...(marginSize !== undefined && { marginSize }),
        ...(lastUpdate !== undefined && { lastUpdate }),
      });
    }
    return rows;
  }
  return undefined;
}
