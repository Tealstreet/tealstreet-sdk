import type { WasmEngine } from './engine.mjs';

/**
 * Loads the engine for a Rust venue: in browsers and Workers only that venue's
 * family build (`yarn rust:build-wasm --families`), in Node the combined build
 * (`yarn rust:build-wasm`). Rejects an unknown venue in browsers.
 */
export function loadEngine(venue: string): Promise<{ WasmEngine: typeof WasmEngine }>;
