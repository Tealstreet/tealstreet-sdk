import type { WasmEngineModule } from './rust.exchange';

import { venueFamily } from '@tealstreet/safe-cex-wasm/venues';
import { createRequire } from 'node:module';

/** The venue-specific wasm engine for Node: servers, the CLI and tests. */
export const loadNodeEngine = async (venue: string): Promise<WasmEngineModule> =>
  createRequire(import.meta.url)(`@tealstreet/safe-cex-wasm/node/${venueFamily(venue)}`);
