import type { Market } from '@tealstreet/safe-cex/src/types';

import { baseToNativeContracts } from '@tealstreet/safe-cex/src/utils/baseToNativeContracts';

/**
 * A USD notional turned into the NATIVE contract count an order takes.
 *
 * Every order verb in the product — `createMarketOrder`, `createLimitOrder`,
 * `createTWAPOrder`, `chaseOrder`, `CoreAdapter.placeOrder` — takes native
 * contracts, and `convertFromNativeSizeToQuote` is its inverse. `notional /
 * price` is only the BASE amount, and the two differ by `contractSize`:
 *
 *   OKX `DOGE-USDT-SWAP` is 1000 DOGE per contract, `BTC-USDT-SWAP` is 0.01 BTC.
 *
 * Linear and inverse need different arithmetic, which is the trap. On an
 * inverse market a contract is a fixed QUOTE amount, so the base amount is not
 * an intermediate step at all — the conversion is `notional / contractSize`
 * and the price never enters. `baseToNativeContracts` says so in its own doc
 * and handles the linear half only; applying it to an inverse market divides
 * by the price for no reason.
 *
 * This existed twice with only the linear half correct in one of them:
 * `TradingBaseCommand.notionalToSize` in the CLI had both branches, and
 * `NewsTradingMarket` had neither and shipped the bare base amount to four
 * order types.
 *
 * Returns the RAW native size. The caller rounds — which rule applies depends
 * on whether it is placing, amending or closing, and those are five different
 * rules (see `orderPricing.ts`).
 */
export const notionalToNativeSize = (notional: number, market: Market, price: number): number => {
  const contractSize = market.contractSize && market.contractSize > 0 ? market.contractSize : 1;
  if (market.isInverse) return notional / contractSize;
  return baseToNativeContracts(market, notional / price);
};
