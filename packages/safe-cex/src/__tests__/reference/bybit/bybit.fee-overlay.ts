import type { Market } from '../../../types';
import type { AccountMarketOverlay } from '../../../exchanges/base';

export type BybitFeeRate = { symbol: string; takerFeeRate: string; makerFeeRate: string };

/** Per-account maker/taker rates; the keyless shared exchange cannot fetch them. */
export const bybitFeeOverlay = (fees: ReadonlyArray<BybitFeeRate>): AccountMarketOverlay | null => {
  const bySymbol = new Map(fees.map((f) => [f.symbol, f]));
  if (bySymbol.size === 0) return null;
  return (markets: Market[]) =>
    markets.map((m) => {
      const fee = bySymbol.get(m.symbol);
      return fee ? { ...m, takerFee: parseFloat(fee.takerFeeRate), makerFee: parseFloat(fee.makerFeeRate) } : m;
    });
};
