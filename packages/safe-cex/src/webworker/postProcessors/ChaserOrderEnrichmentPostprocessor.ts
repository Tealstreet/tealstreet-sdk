import { ActiveChaser, StoreData, Writable } from '../../types';
import { BasePostprocessor } from './BasePostprocessor';

/**
 * Enriches orders with chaser information by checking if an order belongs to an active chaser
 */
export class ChaserOrderEnrichmentPostprocessor extends BasePostprocessor {
  process(store: Writable<StoreData>): void {
    // Build a single orderId → owning-chaser map. The prior shape did the
    // membership check via a Set but then re-scanned `activeChasers` with
    // `find(c => c.currentOrderIds?.includes(order.id))` for each matched
    // order — O(C × O) per matching order. One pass over chasers is enough.
    const chaserByOrderId: Record<string, ActiveChaser> = {};

    for (const chaser of store.activeChasers) {
      const ids = chaser.currentOrderIds;
      if (!ids || ids.length === 0) continue;
      for (let i = 0; i < ids.length; i++) {
        chaserByOrderId[ids[i]] = chaser;
      }
    }

    // Enrich orders with chaser information
    for (const order of store.orders) {
      const ownerChaser = chaserByOrderId[order.id];
      if (ownerChaser) {
        (order as Writable<typeof order>).extra = {
          ...order.extra,
          isFromChaser: true,
          chaserId: ownerChaser.id,
          chaserAttempts: ownerChaser.attempts,
          chaserMaxAttempts: ownerChaser.maxAttempts,
        };
      }
    }
  }
}
