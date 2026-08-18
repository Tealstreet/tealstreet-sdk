import type { TrackedPlacementOutcome } from './types';

/** Stock `placeOrder` resolves to `string[]`. Exactly one nonempty ID is accepted. */
export const classifyPlaceOrderIds = (value: unknown): TrackedPlacementOutcome => {
  if (!Array.isArray(value)) {
    return { kind: 'ambiguous', reason: 'placement-result-not-id-list' };
  }
  const ids = value.filter((id): id is string => typeof id === 'string' && id.length > 0);
  if (value.length === 1 && ids.length === 1) {
    return { kind: 'accepted', venueOrderId: ids[0] };
  }
  if (ids.length === 0) {
    return { kind: 'ambiguous', reason: 'placement-zero-venue-ids' };
  }
  return { kind: 'ambiguous', reason: 'placement-multiple-venue-ids' };
};

export const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

export const nonEmptyString = (value: unknown): string | undefined =>
  typeof value === 'string' && value.length > 0 ? value : undefined;
