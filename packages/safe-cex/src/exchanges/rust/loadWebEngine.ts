import type { WasmEngineModule } from './rust.exchange';

/** The wasm engine for browsers and Workers: only this venue's family build. */
export const loadWebEngine = async (venue: string): Promise<WasmEngineModule> => {
  const { loadEngine } = await import('@tealstreet/safe-cex-wasm/engine/web');
  return loadEngine(venue);
};
