import { Market, OrderSide } from '../../types';
import { TwapStatus } from './TwapStatus';
import type { TwapOrigin } from './TwapOptions';

export interface TwapState {
  accountName: string;
  activeMarket: Market;
  size: number;
  ordersSizes: number[];
  side: OrderSide;
  status: TwapStatus;
  ordersCount: number;
  ordersExecuted: number;
  nextOrderAt: number;
  markForCleanup?: boolean;
  /** Origin of this TWAP - where it was initiated from */
  origin?: TwapOrigin;
  /** Timestamp when the TWAP was created */
  createdAt?: number;
  /** Timestamp when the TWAP started executing */
  startedAt?: number;
  /**
   * The slice being placed, persisted (already shifted out of `ordersSizes`)
   * before its order goes out and cleared once it settles. A restore that finds
   * it looks the order up by `clientOrderId` instead of placing it again.
   */
  inFlight?: { size: number; clientOrderId?: string };
}
