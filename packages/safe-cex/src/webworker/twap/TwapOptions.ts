import { Market, OrderSide } from '../../types';

/**
 * Origin of a TWAP task - where it was initiated from
 */
export type TwapOrigin = 'web' | 'cli' | 'webhook' | 'automation';

export type TwapTimeoutAction = 'market' | 'next';
export type TwapFailAction = 'market' | 'next';

export interface TwapOptions {
  accountName: string;
  activeMarket: Market;
  size: number;
  /** Total TWAP duration in **minutes** (web GUI convention).
   *  TwapInstance multiplies by 60_000 to pace slices. CLI callers must
   *  convert from ms via twapTaskParamsToOptions. */
  duration: number;
  ordersCount: number;
  side: OrderSide;
  chaserDelay: number;
  reduceOnly: boolean;
  randomnessPercentage: number;
  useLimitOrders: boolean;
  useChaserOrders: boolean;
  /**
   * `twap chase` policy: when a slice's chaser hasn't filled by
   * `timeoutPct%` of the slice interval, take this action.
   * - `market`: cancel the chaser and place a market for the slice's
   *   remaining size.
   * - `next`: cancel the chaser and skip ahead to the next slice
   *   (forfeit the unfilled portion of this slice).
   *
   * Default per spec is `market` at `50%`. Only consulted when
   * `useChaserOrders` is true.
   */
  timeoutPct?: number;
  timeoutAction?: TwapTimeoutAction;
  /**
   * `twap chase` policy: when the slice's chaser stops without
   * filling (cancelled / maxed out / errored), take this action on
   * the next slice tick. Default `market`.
   */
  failAction?: TwapFailAction;
  /** Origin of this TWAP - where it was initiated from */
  origin?: TwapOrigin;
}
