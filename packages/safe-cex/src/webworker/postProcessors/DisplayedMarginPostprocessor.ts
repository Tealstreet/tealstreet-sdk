import { PositionSide, StoreData, Writable } from '../../types';
import { BasePostprocessor } from './BasePostprocessor';

/** Aggregates displayed exposures in source order after position processing.
 * Unknown notional makes the affected side and total unknown.
 */
export class DisplayedMarginPostprocessor extends BasePostprocessor {
  process(store: Writable<StoreData>): void {
    const positions = store.displayedPositions || [];

    let longExposure = 0;
    let shortExposure = 0;
    let totalNotional = 0;

    for (let i = 0; i < positions.length; i++) {
      const value = positions[i].notional;
      const notional = typeof value === 'number' && Number.isFinite(value) ? Math.abs(value) : NaN;
      totalNotional += notional;
      if (positions[i].side === PositionSide.Long) {
        longExposure += notional;
      } else if (positions[i].side === PositionSide.Short) {
        shortExposure += notional;
      }
    }

    store.displayedMargin = {
      longExposure: Number.isFinite(longExposure) ? longExposure : null,
      shortExposure: Number.isFinite(shortExposure) ? shortExposure : null,
      totalNotional: Number.isFinite(totalNotional) ? totalNotional : null,
    };
  }
}
