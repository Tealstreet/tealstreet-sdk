import type { WasmEngine } from './engine.mjs';

export interface EngineModule {
  WasmEngine: typeof WasmEngine;
  wasmMemoryBytes(): number;
}
/** Loads only the runtime's venue family, built by `yarn rust:build-wasm`. */
export function loadEngine(venue: string): Promise<EngineModule>;
