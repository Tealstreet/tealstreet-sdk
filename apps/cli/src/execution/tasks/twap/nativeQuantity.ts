import type { Market } from '@tealstreet/safe-cex/src/types';
import { baseToNativeContracts } from '@tealstreet/safe-cex/src/utils/baseToNativeContracts';

/** Resolve the CLI TWAP size into the contract count expected by the legacy engine. */
export const twapTotalInNativeContracts = (
  market: Pick<Market, 'contractSize' | 'isInverse'>,
  totalSize: number,
  sizeIsNative: boolean,
): number => {
  if (sizeIsNative) return totalSize;
  if (market.isInverse === true) {
    throw new Error('Bare base quantities are not supported for inverse TWAP markets');
  }
  if (market.contractSize !== undefined && (!Number.isFinite(market.contractSize) || market.contractSize <= 0)) {
    throw new Error('Cannot convert TWAP base quantity: invalid contract size');
  }
  return baseToNativeContracts(market, totalSize);
};
