// Loads the Node build of the wasm engine (built by `harness/build.sh`).
import { createRequire } from 'node:module';

export const loadEngine = async () => {
  const require = createRequire(import.meta.url);
  return require('../safe-cex-wasm/pkg/node/safe_cex_wasm.js');
};
