// Loads the Node build of the wasm engine (built by `harness/build.sh`);
// `path` selects another build, such as a venue family package.
import { createRequire } from 'node:module';
import { resolve } from 'node:path';

export const loadEngine = async (path) => {
  const require = createRequire(import.meta.url);
  return require(path ? resolve(path) : '../hosts/wasm/pkg/node/safe_cex_wasm.js');
};
