import type { Market, OrderSide } from '@tealstreet/safe-cex/src/types';
import type {
  TrackedLookupResult,
  TrackedPlacementOutcome,
} from '@tealstreet/safe-cex/src/tracked';

export type ExactQuantity = {
  readonly decimal: string;
  readonly atoms: string;
  readonly atomScale: number;
};

export type LogicalOrderIdentity = {
  readonly ownerId: string;
  readonly unitId: string;
};

export type ConfirmedMarketOrder = {
  readonly market: Market;
  readonly side: OrderSide;
  readonly reduceOnly?: boolean;
};

export type ConfirmedOrderHandle = {
  readonly identity: LogicalOrderIdentity;
  readonly fingerprint: string;
  readonly marketId: string;
  readonly symbol: string;
  readonly callerId: string;
  readonly wireClientOrderId: string;
  readonly exact: ExactQuantity;
  readonly placement: TrackedPlacementOutcome;
  readonly venueOrderId?: string;
};

export type OpenOrderTruthProjection = {
  readonly status: 'open' | 'partially_filled';
  readonly venueOrderId?: string;
  readonly total: ExactQuantity;
  readonly filled: ExactQuantity;
  readonly residual: ExactQuantity;
  readonly averagePrice?: string;
  readonly raw: unknown;
  readonly lastIndeterminateReason?: string;
};

export type TerminalOrderTruthProjection = {
  readonly status: 'filled' | 'canceled' | 'rejected' | 'expired' | 'not_placed';
  readonly venueOrderId?: string;
  readonly total: ExactQuantity;
  readonly filled: ExactQuantity;
  readonly residual: ExactQuantity;
  readonly averagePrice?: string;
  readonly raw?: unknown;
  readonly lastIndeterminateReason?: string;
};

export type OrderTruthProjection = OpenOrderTruthProjection | TerminalOrderTruthProjection;

export type LastGoodOrderTruth =
  | {
      readonly kind: 'unconfirmed';
      readonly handle: ConfirmedOrderHandle;
      readonly lastIndeterminateReason?: string;
    }
  | {
      readonly kind: 'open';
      readonly handle: ConfirmedOrderHandle;
      readonly observation: OpenOrderTruthProjection;
    }
  | {
      readonly kind: 'terminal';
      readonly handle: ConfirmedOrderHandle;
      readonly observation: TerminalOrderTruthProjection;
    };

export type OrderTruth =
  | LastGoodOrderTruth
  | {
      readonly kind: 'incident';
      readonly handle: ConfirmedOrderHandle;
      readonly reason: string;
      readonly offendingRaw?: unknown;
      readonly lastGood: LastGoodOrderTruth;
    };

export type ConfirmedAuditEvent =
  | {
      readonly kind: 'confirmed-place';
      readonly ownerId: string;
      readonly unitId: string;
      readonly callerId: string;
      readonly wireClientOrderId: string;
      readonly marketId: string;
      readonly symbol: string;
      readonly side: OrderSide;
      readonly reduceOnly: boolean;
      readonly exact: ExactQuantity;
    }
  | {
      readonly kind: 'confirmed-placed';
      readonly ownerId: string;
      readonly unitId: string;
      readonly callerId: string;
      readonly wireClientOrderId: string;
      readonly placement: TrackedPlacementOutcome;
      readonly venueOrderId?: string;
    };

export type ConfirmedAuditSink = {
  emit(event: ConfirmedAuditEvent): void;
};

export type { TrackedLookupResult, TrackedPlacementOutcome };
