/**
 * Stub implementations for build-time resolution.
 * Real implementations are loaded dynamically at runtime.
 */

import type { PoseidonCrypto } from './types';

export function initWasm(): Promise<void> {
  throw new Error('WASM not loaded. This is a stub function.');
}

export function isWasmReady(): boolean {
  return false;
}

export function getCryptoApi(): PoseidonCrypto {
  throw new Error('WASM not loaded. This is a stub function.');
}