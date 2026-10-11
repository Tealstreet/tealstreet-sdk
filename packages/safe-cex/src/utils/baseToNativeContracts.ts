import type { Market } from '../types';

/**
 * Convert a **base-denominated** amount into the **native contract count** a
 * linear venue expects as its order size.
 *
 * Most linear venues really do use 1 contract = 1 base coin, so this is a no-op
 * for bybit / bitget and the rest. OKX is the exception: its linear
 * swaps publish a real `ctVal` — `DOGE-USDT-SWAP` is 1000 DOGE per contract,
 * `BTC-USDT-SWAP` is 0.01 BTC — so sending a base amount straight through is
 * wrong by a factor of `contractSize`, in whichever direction `ctVal` points:
 *
 *   DOGE  $10 → 142.86 base → sent as 142.86 contracts = $10,000 notional
 *   BTC   0.0001 base → adjust(0.0001, lotSz 0.01) → 0 → "must be finite and positive"
 *
 * `OkxExchange.contractsToBase` is the read-side counterpart; this is its
 * inverse, and the two must stay symmetric.
 *
 * **Inverse markets are not handled here.** There a contract is a fixed *quote*
 * amount, so the conversion is `notional / contractSize` and the caller needs
 * the notional rather than a base amount — see `TradingBaseCommand.notionalToSize`.
 */
export const baseToNativeContracts = (market: Pick<Market, 'contractSize'>, baseAmount: number): number => {
  const contractSize = market.contractSize && market.contractSize > 0 ? market.contractSize : 1;
  return baseAmount / contractSize;
};
