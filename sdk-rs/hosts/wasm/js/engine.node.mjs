// CommonJS initialization is cached by Node and shared with the explicit loader.
import { createRequire } from 'node:module';
import { venueFamily } from './venues.mjs';

const require = createRequire(import.meta.url);
export async function loadEngine(venue) {
  return require(`../pkg/node/${venueFamily(venue)}/safe_cex_wasm.js`);
}
