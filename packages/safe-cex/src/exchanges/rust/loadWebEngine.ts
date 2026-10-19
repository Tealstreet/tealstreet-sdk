import type { VenueFamily } from '@tealstreet/safe-cex-wasm/venues';
import type { WasmEngineModule } from './rust.exchange';

import { venueFamily } from '@tealstreet/safe-cex-wasm/venues';

const pending = new Map<VenueFamily, Promise<WasmEngineModule>>();

// Literal imports keep each venue in its own browser chunk.
const importEngine = (family: VenueFamily) => {
  switch (family) {
    case 'binance': return import('@tealstreet/safe-cex-wasm/web/binance');
    case 'bybit': return import('@tealstreet/safe-cex-wasm/web/bybit');
    case 'cryptocom': return import('@tealstreet/safe-cex-wasm/web/cryptocom');
    case 'okx': return import('@tealstreet/safe-cex-wasm/web/okx');
  }
};

/** Loads only this venue's engine, shared by accounts and aliases in this realm. */
export const loadWebEngine = (venue: string): Promise<WasmEngineModule> => {
  const family = venueFamily(venue);
  let loading = pending.get(family);
  if (!loading) {
    loading = importEngine(family).then(async (engine) => {
      await engine.default();
      return engine;
    }).catch((error) => {
      pending.delete(family);
      throw error;
    });
    pending.set(family, loading);
  }
  return loading;
};
