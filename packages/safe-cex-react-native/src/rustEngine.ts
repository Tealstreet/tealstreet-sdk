import type { WasmEngineModule } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';

/**
 * Await this registration before creating any Rust account. A host
 * with WebAssembly can initialize the browser artifact from bundled bytes;
 * Hermes needs a native bridge exposing the same engine interface. No browser
 * fetch, filesystem loader or global WebAssembly assumption lives here.
 */
export const setReactNativeRustEngineLoader = async (loader: () => Promise<WasmEngineModule>) => {
  const { setWasmEngineLoader } = await import('@tealstreet/safe-cex/src/exchanges/rust/rust.exchange');
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
