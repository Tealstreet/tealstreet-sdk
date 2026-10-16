import type { Market } from '@tealstreet/safe-cex/src/types';
import { multiply } from '@tealstreet/safe-cex/src/utils/safe-math';
import { formatSizeCompact } from '@tealstreet/utils/src/formatting';

/**
 * SDK position.contracts and order.remaining are base amounts, but a market's
 * amount precision is its native lot step. Linear lots have a fixed base step;
 * inverse lots do not, because their base value changes with price.
 */
export function formatBaseSize(size: number, market?: Market | null): string {
  const baseStep =
    market && !market.isInverse ? multiply(market.precision.amount, market.contractSize ?? 1) : undefined;
  return formatSizeCompact(Math.abs(size), baseStep);
}
