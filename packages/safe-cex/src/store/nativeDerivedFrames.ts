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

/** Decode each section independently so one rejected section cannot hide a peer. */
export function readNativeDerivedSections(
  frame: NativeDerivedFrame,
  accept: <K extends keyof StoreData>(key: K, value: StoreData[K]) => void,
  reject: (error: unknown) => void,
): void {
  const read = <K extends keyof StoreData>(key: K, decode: () => StoreData[K] | undefined) => {
    try {
      const value = decode();
      if (value !== undefined) accept(key, value);
    } catch (error) { reject(error); }
  };
  read('socketHealth', () => frame.socketHealth?.());
  read('activeChasers', () => frame.activeChasers?.());
  read('activeTWAPs', () => frame.activeTWAPs?.());
  read('ordersRisk', () => frame.ordersRisk?.());
  read('displayedPositions', () => frame.displayedPositions?.());
  read('displayedMargin', () => frame.displayedMargin?.());
}
