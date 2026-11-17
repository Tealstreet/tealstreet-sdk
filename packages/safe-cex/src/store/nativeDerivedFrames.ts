import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { StoreData } from '../generated/StoreData';

/** Decode each section independently so one rejected section cannot hide a peer. */
export function readNativeDerivedSections(
  frame: Frame,
  accept: <K extends keyof StoreData>(key: K, value: StoreData[K]) => void,
  reject: (error: unknown) => void,
): void {
  const read = <K extends keyof StoreData>(key: K, decode: () => StoreData[K] | undefined) => {
    try {
      const value = decode();
      if (value !== undefined) accept(key, value);
    } catch (error) {
      reject(error);
    }
  };
  read('socketHealth', () => frame.socketHealth?.());
  read('activeChasers', () => frame.activeChasers?.());
  read('activeTWAPs', () => frame.activeTWAPs?.());
  read('ordersRisk', () => frame.ordersRisk?.());
  read('displayedPositions', () => frame.displayedPositions?.());
  read('displayedMargin', () => frame.displayedMargin?.());
}
