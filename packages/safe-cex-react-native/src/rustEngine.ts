import type { WasmEngineModule } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import type { VenueFamily } from '@tealstreet/safe-cex-wasm/venues';

import { venueFamily } from '@tealstreet/safe-cex-wasm/venues';

/**
 * Await this registration before creating any Rust account. A host
 * with WebAssembly can initialize the browser artifact from bundled bytes;
 * Hermes needs a native bridge exposing the same engine interface. No browser
 * fetch, filesystem loader or global WebAssembly assumption lives here.
 */
export const setReactNativeRustEngineLoader = async (loader: (family: VenueFamily) => Promise<WasmEngineModule>) => {
  const { setWasmEngineLoader } = await import('@tealstreet/safe-cex/src/exchanges/rust/rust.exchange');
  const pending = new Map<VenueFamily, Promise<WasmEngineModule>>();
  setWasmEngineLoader((venue) => {
    const family = venueFamily(venue);
    let loading = pending.get(family);
    if (!loading) {
      loading = Promise.resolve()
        .then(() => loader(family))
        .catch((error) => {
          if (pending.get(family) === loading) pending.delete(family);
          throw error;
        });
      pending.set(family, loading);
    }
    return loading;
  });
};
