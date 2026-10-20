import type { WasmEngine } from './engine.mjs';

/** Loads the runtime's generated artifact, built by `yarn rust:build-wasm`. */
export function loadEngine(): Promise<{ WasmEngine: typeof WasmEngine }>;
