import type { WasmEngineModule } from './rust.exchange';

import { createRequire } from 'node:module';

/** The wasm engine for Node: servers, the CLI and tests. */
export const loadNodeEngine = async (): Promise<WasmEngineModule> =>
  createRequire(import.meta.url)('@tealstreet/safe-cex-wasm/node');
