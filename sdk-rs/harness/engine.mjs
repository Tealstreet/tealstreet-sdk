// Loads only the requested venue's Node build (built by `harness/build.sh`).
import { createRequire } from 'node:module';
import { venueFamily } from '../hosts/wasm/js/venues.mjs';

export const loadEngine = async (venue) => {
  const require = createRequire(import.meta.url);
  return require(`../hosts/wasm/pkg/node/${venueFamily(venue)}/safe_cex_wasm.js`);
};
