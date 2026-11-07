import { StoreData, Writable } from '../../types';
import { BasePostprocessor } from './BasePostprocessor';

/**
 * Enriches direct TWAP limit orders so UI consumers can avoid manual edits on
 * orders still controlled by a running TWAP instance.
 */
export class TwapOrderEnrichmentPostprocessor extends BasePostprocessor {
  process(store: Writable<StoreData>): void {
    const twapByOrderId: Record<string, { id: string }> = {};

    for (const twap of store.activeTWAPs) {
      const ids = (twap as { currentOrderIds?: string[] }).currentOrderIds;
      if (!ids || ids.length === 0) continue;
      for (let i = 0; i < ids.length; i++) {
        twapByOrderId[ids[i]] = twap;
      }
    }

    for (const order of store.orders) {
      const ownerTwap = twapByOrderId[order.id];
      if (!ownerTwap) continue;

      (order as Writable<typeof order>).extra = {
        ...order.extra,
        isFromTwap: true,
        twapId: ownerTwap.id,
      } as typeof order.extra;
    }
  }
}
