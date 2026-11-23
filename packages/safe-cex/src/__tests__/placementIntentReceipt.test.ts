import { expect, it } from 'vitest';

import { canRetirePlacementReceipt } from '../placementIntentReceipt';

it('retains empty or malformed receipts as uncertain, including a wrapped caller-ID result', () => {
  for (const result of [[], [''], [null], { orderIds: [], clientOrderId: 'caller' }, null, { success: true }])
    expect(canRetirePlacementReceipt(result)).toBe(false);
});

it('allows known order IDs and the existing explicit protection-only completion marker to retire', () => {
  const protection = Object.assign([], { protectionApplied: true });
  for (const result of [['venue-order'], { orderIds: ['venue-order'], clientOrderId: 'caller' }, protection])
    expect(canRetirePlacementReceipt(result)).toBe(true);
});
