import type { WasmEngineModule } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';

import { setWasmEngineLoader } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';

/**
 * Register the mobile host's engine before creating any Rust account. A host
 * with WebAssembly can initialize the browser artifact from bundled bytes;
 * Hermes needs a native bridge exposing the same engine interface. No browser
 * fetch, filesystem loader or global WebAssembly assumption lives here.
 */
export const setReactNativeRustEngineLoader = (loader: () => Promise<WasmEngineModule>) => {
  let pending: Promise<WasmEngineModule> | undefined;
  setWasmEngineLoader(
    () =>
      (pending ??= Promise.resolve()
        .then(loader)
        .catch((error) => {
          pending = undefined;
          throw error;
        })),
  );
};
