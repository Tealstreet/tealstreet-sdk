// Bundled beside dist/index.js with the same banner and run by `yarn smoke`:
// Lighter signs through this WASM, and only a bundled run proves it loads.
import { getCryptoApi, initWasm } from '@tealstreet/poseidon-crypto/src/index';

await initWasm();
const keys = getCryptoApi().schnorrGenerateKeys();
if (!('publicKey' in keys))
  throw new Error(`poseidon WASM loaded but could not generate keys: ${JSON.stringify(keys)}`);
console.log('poseidon: WASM loaded and signing keys generated');
process.exit(0);
