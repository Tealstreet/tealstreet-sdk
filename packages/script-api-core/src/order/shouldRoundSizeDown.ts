import { ExchangeName, Market } from '@tealstreet/safe-cex/src/types';

/**
 * Whether an order size should be rounded DOWN to the lot step rather than to
 * the nearest step.
 *
 * Rounding down on spot avoids asking for slightly more base than the wallet
 * actually holds, which Hyperliquid rejects. It is deliberately scoped to
 * Hyperliquid rather than "any spot market": several other live venues
 * (Coinbase, Backpack, KuCoin Spot) also expose `isSpot` markets and have
 * always rounded to nearest, and truncating their sizes would silently drop up
 * to a full lot on every order — a money-affecting change that belongs in its
 * own reviewed PR, not as a side effect of the Hyperliquid spot merge.
 *
 * The check is (venue AND market), not exchange name alone, because a single
 * `hyperliquid` connection now serves both spot and perp markets — perps on
 * that same connection must keep rounding to nearest.
 */
const ROUND_DOWN_SPOT_VENUES: ReadonlySet<string> = new Set<ExchangeName>(['hyperliquid', 'hyperliquidspot']);

export const shouldRoundSizeDown = (market: Pick<Market, 'isSpot'>, exchangeName?: string): boolean =>
  Boolean(market?.isSpot) && Boolean(exchangeName) && ROUND_DOWN_SPOT_VENUES.has(exchangeName as string);
