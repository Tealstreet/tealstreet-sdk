import { PositionSide, StoreData, Writable } from '../../types';
import { BasePostprocessor } from './BasePostprocessor';

/**
 * Derives per-account margin aggregation from the post-processed positions so
 * the margin UI reads it off the store instead of recomputing per render.
 *
 * Runs after DisplayedPositionsPostprocessor and aggregates over
 * `store.displayedPositions` with the exact formula the margin UI used inline
 * (`Math.abs(notional || 0)` summed in array order, per side and overall). The
 * single pass preserves array order, so the long/short/total sums are
 * floating-point identical to the previous filter+reduce + total loop.
 */
export class DisplayedMarginPostprocessor extends BasePostprocessor {
  process(store: Writable<StoreData>): void {
    const positions = store.displayedPositions || [];

    let longExposure = 0;
    let shortExposure = 0;
    let totalNotional = 0;

    for (let i = 0; i < positions.length; i++) {
      const notional = Math.abs(positions[i].notional || 0);
      totalNotional += notional;
      if (positions[i].side === PositionSide.Long) {
        longExposure += notional;
      } else if (positions[i].side === PositionSide.Short) {
        shortExposure += notional;
      }
    }

    store.displayedMargin = { longExposure, shortExposure, totalNotional };
  }
}
