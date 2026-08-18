import type { Exchange } from '../exchanges/base';
import type { Market } from '../types';

export type TrackedPlacementOutcome =
  | { readonly kind: 'accepted'; readonly venueOrderId: string }
  | { readonly kind: 'rejected'; readonly reason: string }
  | { readonly kind: 'ambiguous'; readonly reason: string };

export type TrackedLookupStatus = 'open' | 'partially_filled' | 'filled' | 'canceled' | 'rejected' | 'expired';

export type TrackedLookupResult =
  | {
      readonly kind: 'found';
      readonly wireClientOrderId: string;
      readonly venueOrderId: string;
      readonly status: TrackedLookupStatus;
      readonly totalAmount: string;
      readonly cumulativeFilledAmount: string;
      readonly averagePrice?: string;
      readonly raw: unknown;
    }
  | { readonly kind: 'confirmed_absent'; readonly raw: unknown }
  | { readonly kind: 'indeterminate'; readonly reason: string; readonly raw?: unknown };

export type TrackedPlacementInput =
  | { readonly kind: 'resolved'; readonly value: unknown }
  | { readonly kind: 'thrown'; readonly error: unknown };

export interface TrackedRegularOrderCapability {
  readonly market: Market;
  composeClientOrderId(callerId: string): string;
  classifyPlacement(input: TrackedPlacementInput): TrackedPlacementOutcome;
  lookupExact(input: { readonly wireClientOrderId: string }): Promise<TrackedLookupResult>;
}

export type TrackedExactOrderReader = {
  getOrderByClientOrderId(symbol: string, wireClientOrderId: string): Promise<unknown>;
};

export type TrackedCapabilityFactory = (
  exchange: Exchange,
  market: Market,
) => TrackedRegularOrderCapability | undefined;
