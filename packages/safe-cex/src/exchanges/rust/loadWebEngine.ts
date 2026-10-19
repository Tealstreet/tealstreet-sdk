import type { VenueFamily } from '@tealstreet/safe-cex-wasm/venues';
import type { WasmEngineModule } from './rust.exchange';

import { venueFamily } from '@tealstreet/safe-cex-wasm/venues';

const pending = new Map<VenueFamily, Promise<WasmEngineModule>>();

// Literal imports keep each venue in its own browser chunk.
const importEngine = (family: VenueFamily) => {
  switch (family) {
    case 'backpack': return import('@tealstreet/safe-cex-wasm/web/backpack');
    case 'binance': return import('@tealstreet/safe-cex-wasm/web/binance');
    case 'bingx': return import('@tealstreet/safe-cex-wasm/web/bingx');
    case 'blofin': return import('@tealstreet/safe-cex-wasm/web/blofin');
    case 'bybit': return import('@tealstreet/safe-cex-wasm/web/bybit');
    case 'cryptocom': return import('@tealstreet/safe-cex-wasm/web/cryptocom');
    case 'mexc': return import('@tealstreet/safe-cex-wasm/web/mexc');
    case 'okx': return import('@tealstreet/safe-cex-wasm/web/okx');
    case 'weex': return import('@tealstreet/safe-cex-wasm/web/weex');
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
