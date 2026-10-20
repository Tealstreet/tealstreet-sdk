// The Node artifact owns its fs-based initialization; never load it in browsers.
// Servers host many venues, so Node keeps the combined engine for every venue.
export function loadEngine() {
  return import('../pkg/node/safe_cex_wasm.js');
}
