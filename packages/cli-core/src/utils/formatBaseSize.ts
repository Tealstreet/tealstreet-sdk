import type { Market, Order } from '@tealstreet/safe-cex/src/types';
import { divide, multiply } from '@tealstreet/safe-cex/src/utils/safe-math';
import { formatSizeCompact } from '@tealstreet/utils/src/formatting';

/**
 * A market's amount precision is its native lot step. Linear lots have a fixed
 * base step; inverse lots do not, because their base value changes with price.
 */
export function formatBaseSize(size: number, market?: Market | null): string {
  if (market?.isInverse && Math.abs(size) < 1) {
    return Math.abs(size).toLocaleString('en-US', { useGrouping: false, maximumSignificantDigits: 8 });
  }
  const baseStep =
    market && !market.isInverse ? multiply(market.precision.amount, market.contractSize ?? 1) : undefined;
  return formatSizeCompact(Math.abs(size), baseStep);
}

// Inverse order.remaining may be contracts (COIN-M) or base (OKX/Bybit).
// The SDK's remainingNotional includes the contract multiplier for either form.
export function formatOrderBaseSize(order: Order, market?: Market | null): string {
  const notional = order.extra?.remainingNotional;
  if (market?.isInverse && notional !== undefined) {
    if (!Number.isFinite(notional) || notional < 0 || !Number.isFinite(order.price) || order.price <= 0) return '-';
    return formatBaseSize(divide(notional, order.price), market);
  }
  return formatBaseSize(order.remaining, market);
}
