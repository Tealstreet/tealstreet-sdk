import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { StoreData } from '../generated/StoreData';

/** Declaration boundary while the native wire sections are being added. */
export interface NativeDerivedFrame extends Frame {
  socketHealth?(): StoreData['socketHealth'];
  activeChasers?(): StoreData['activeChasers'] | undefined;
  activeTWAPs?(): StoreData['activeTWAPs'] | undefined;
  ordersRisk?(): StoreData['ordersRisk'] | undefined;
  displayedPositions?(): StoreData['displayedPositions'] | undefined;
  displayedMargin?(): StoreData['displayedMargin'];
  balanceInfo?(): StoreData['balance']['info'];
}

/** Native sections decode directly to generated types. Absence preserves state. */
export function readNativeDerivedSections(frame: NativeDerivedFrame): Partial<StoreData> {
  const socketHealth = frame.socketHealth?.();
  const activeChasers = frame.activeChasers?.();
  const activeTWAPs = frame.activeTWAPs?.();
  const ordersRisk = frame.ordersRisk?.();
  const displayedPositions = frame.displayedPositions?.();
  const displayedMargin = frame.displayedMargin?.();
  return {
    ...(socketHealth !== undefined && { socketHealth }),
    ...(activeChasers !== undefined && { activeChasers }),
    ...(activeTWAPs !== undefined && { activeTWAPs }),
    ...(ordersRisk !== undefined && { ordersRisk }),
    ...(displayedPositions !== undefined && { displayedPositions }),
    ...(displayedMargin !== undefined && { displayedMargin }),
  };
}
