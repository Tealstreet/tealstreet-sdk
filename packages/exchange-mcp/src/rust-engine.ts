import type { WasmEngineModule } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';

import { createRequire } from 'node:module';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { setWasmEngineLoader } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import { venueFamily } from '@tealstreet/safe-cex-wasm/venues';

/** Bundled releases own their engine beside index.js, independent of a workspace. */
export const registerBundledRustEngine = () => {
  const bundled = new URL('./wasm/rust/', import.meta.url);
  if (!existsSync(bundled)) return;
  const require = createRequire(import.meta.url);
  const pending = new Map<string, Promise<WasmEngineModule>>();
  const load = (venue: string) => {
    const family = venueFamily(venue);
    let loading = pending.get(family);
    if (!loading) {
      loading = Promise.resolve()
        .then(() => require(fileURLToPath(new URL(`${family}/safe_cex_wasm.cjs`, bundled))))
        .catch((error: unknown) => {
          if (pending.get(family) === loading) pending.delete(family);
          throw error;
        });
      pending.set(family, loading);
    }
    return loading;
  };
  setWasmEngineLoader(load);
  return load;
};
