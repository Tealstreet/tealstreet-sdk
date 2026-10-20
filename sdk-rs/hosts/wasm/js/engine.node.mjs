// The Node artifact owns its fs-based initialization; never load it in browsers.
export function loadEngine() {
  return import('../pkg/node/safe_cex_wasm.js');
}
