import type { WasmEngineModule } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';

import { createRequire } from 'node:module';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { setWasmEngineLoader } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';

/** Bundled releases own their engine beside index.js, independent of a workspace. */
export const registerBundledRustEngine = () => {
  const bundled = new URL('./wasm/rust/safe_cex_wasm.cjs', import.meta.url);
  if (!existsSync(bundled)) return;
  const require = createRequire(import.meta.url);
  let pending: Promise<WasmEngineModule> | undefined;
  setWasmEngineLoader(
    () =>
      (pending ??= Promise.resolve()
        .then(() => require(fileURLToPath(bundled)))
        .catch((error: unknown) => {
          pending = undefined;
          throw error;
        })),
  );
};
