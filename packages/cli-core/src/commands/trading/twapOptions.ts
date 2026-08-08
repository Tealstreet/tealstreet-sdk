import type { Market, OrderSide } from '@tealstreet/safe-cex/src/types';
import type { TwapOptions } from '@tealstreet/safe-cex/src/webworker/twap/TwapOptions';
import type { TwapTaskParams } from '../../types';

/**
 * Bridge the CLI task convention (TwapTaskParams.duration in ms) to the
 * safe-cex runtime convention (TwapOptions.duration in minutes —
 * TwapInstance multiplies by 60_000 when pacing slices).
 */
export function twapTaskParamsToOptions(
  params: TwapTaskParams,
  ctx: { accountName: string; market: Market; side: OrderSide },
): TwapOptions {
  return {
    accountName: ctx.accountName,
    activeMarket: ctx.market,
    size: params.totalSize,
    duration: params.duration / 60_000, // ms → minutes
    ordersCount: params.slices,
    side: ctx.side,
    chaserDelay: 1000,
    reduceOnly: params.reduceOnly ?? false,
    randomnessPercentage: 0,
    useLimitOrders: false,
    useChaserOrders: params.useChaserOrders ?? false,
    timeoutPct: params.timeoutPct,
    timeoutAction: params.timeoutAction,
    failAction: params.failAction,
    origin: 'cli',
  };
}
