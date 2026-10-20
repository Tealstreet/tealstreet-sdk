/**
 * How a position's size becomes the number an order's `amount` is in.
 *
 * `contracts` is documented as "always in whole units of base"; an order's
 * `amount` is in the VENUE's units. They differ by `contractSize`, and on
 * OKX's `contractSize 0.01` markets they differ a hundredfold — in the
 * direction that silently closes almost nothing.
 *
 * It lives in safe-cex rather than in a host because native bulk closes and the CLI need the
 * same answer: `close` and a `%pos` size both took `position.contracts`
 * straight from the store, which is base, and handed it to `placeOrder` as a
 * native amount. That is the same defect, one runtime over.
 *
 * This existed TWICE. `getPositionNativeSize` (the UI path) and
 * `getPositionNativeContracts` in `order/createMarketOrder.ts` (the full-close
 * check and the AI order path) each stated it, and the second one's own
 * comment said it "Mirrors `getPositionNativeSize`" and that "a copy would
 * drift, and this is the number that must not". It had drifted:
 * `getPositionNativeSize` prefers `Math.max(fromExtra, derived)` while the
 * copy returned `fromExtra` outright, so a stale `extra.contractsNative` of
 * 0.1 against a real 0.4 made the AI path size a close at a QUARTER.
 *
 * Neither branch is provably right and the trade is recorded in
 * `packages/safe-cex/src/exchanges/okx/CLAUDE.md`: `Math.max` risks oversizing
 * on a briefly-inflated notional, `fromExtra` risks under-closing on a stale
 * one. `Math.max` is what already ships on the UI path and what the
 * `getPositionNativeSize.test.ts` case "prefers inverse notional sizing when
 * contractsNative is stale" pins, so the two agreeing on it is a consistency
 * fix rather than new exposure. Change it here and both move together.
 *
 * Rounding is deliberately NOT here. `getPositionNativeSize` rounds to the lot
 * step because its callers feed a UI; the full-close check compares against an
 * `amount` that is already rounded, with a whole-step tolerance that dwarfs
 * the difference.
 */

export interface NativeSizeInputs {
  readonly contracts: number;
  readonly notional?: number;
  readonly extra?: { readonly contractsNative?: number };
}

export interface NativeSizeMarket {
  readonly contractSize?: number;
  readonly isInverse?: boolean;
}

export const nativePositionSize = (position: NativeSizeInputs, market: NativeSizeMarket): number => {
  const fromExtra = Math.abs(Number(position.extra?.contractsNative || 0));

  // `|| 1` — a missing or zero contract size means one-to-one, so there is no
  // divide-by-zero to guard. Both of the previous copies carried a
  // `contractSize > 0` ternary that this makes unreachable, and they disagreed
  // about what its dead branch returned.
  const contractSize = market.contractSize || 1;

  // An inverse contract is a fixed QUOTE amount, so its count comes from the
  // notional and the price never enters. A linear one counts base.
  const source = market.isInverse ? position.notional : position.contracts;

  return Math.max(fromExtra, Math.abs(source || 0) / contractSize);
};
