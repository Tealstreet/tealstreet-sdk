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
}
