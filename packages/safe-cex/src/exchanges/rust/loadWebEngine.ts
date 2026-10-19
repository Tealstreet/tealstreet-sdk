import type { WasmEngineModule } from './rust.exchange';

/** The wasm engine for browsers and Workers; fetches its module once. */
export const loadWebEngine = async (): Promise<WasmEngineModule> => {
  const engine = await import('@tealstreet/safe-cex-wasm/web');
  await engine.default();
  return engine;
};
