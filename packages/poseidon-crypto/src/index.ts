/**
 * @tealstreet/poseidon-crypto
 *
 * WebAssembly-powered Poseidon2 cryptography library
 *
 * Features:
 * - Poseidon2 hash functions
 * - Schnorr signatures over ECgFp5 curve
 * - TypeScript support
 *
 * Usage:
 * ```typescript
 * import { initWasm, getCryptoApi } from '@tealstreet/poseidon-crypto';
 *
 * // Initialize WASM (call once at app startup)
 * await initWasm();
 *
 * // Get the API
 * const crypto = getCryptoApi();
 *
 * // Hash data
 * const hash = crypto.poseidon2Hash("0000000000000000000000000000000000000000000000000000000000000000");
 *
 * // Generate keys
 * const keys = crypto.schnorrGenerateKeys();
 *
 * // Sign a message
 * const signature = crypto.schnorrSign("0000000000000000000000000000000000000000000000000000000000000001", keys.privateKey);
 *
 * // Verify signature
 * const isValid = crypto.schnorrVerify("0000000000000000000000000000000000000000000000000000000000000001", keys.publicKey, signature);
 * ```
 */

export type { Poseidon2Hasher, PoseidonCrypto, SchnorrKeyPair, SchnorrSigner, WasmError } from './types';

// Use a runtime-loaded module
let runtimeModule: any = null;
let initPromise: Promise<void> | null = null;

function isBrowserLikeRuntime(): boolean {
  // Browser main thread.
  if (typeof window !== 'undefined') return true;
  // Any Web Worker (classic OR module): self is a WorkerGlobalScope instance.
  // We read the constructor off globalThis (it's a global in worker scopes,
  // absent in Node) — `WorkerGlobalScope` isn't in the DOM lib's types, so the
  // `any` cast keeps this type-clean while the instanceof check stays correct.
  // Note: importScripts is classic-worker-only, so it can't be relied on here.
  const workerScopeCtor = (globalThis as any).WorkerGlobalScope;
  return typeof workerScopeCtor !== 'undefined' && typeof self !== 'undefined' && self instanceof workerScopeCtor;
}

/**
 * Initialize the WASM module
 * @returns Promise that resolves when WASM is ready
 */
export async function initWasm(): Promise<void> {
  if (runtimeModule) {
    return;
  }

  if (initPromise) {
    return initPromise;
  }

  initPromise = (async () => {
    try {
      // Use literal import specifiers so bundlers (vitest/vite, tsup, Next) can
      // statically resolve both loaders; the unused branch is code-split out. A
      // variable specifier (`await import(loaderPath)`) is not statically
      // resolvable under vite and broke @tealstreet/lighter-sdk's WASM init when
      // poseidon-crypto is imported across the package boundary in tests.
      runtimeModule = isBrowserLikeRuntime() ? await import('./browser-loader') : await import('./node-loader');
      await runtimeModule.initWasm();
    } catch (error) {
      initPromise = null;
      throw new Error(`Failed to initialize WASM: ${error}`);
    }
  })();

  return initPromise;
}

/**
 * Check if WASM is ready
 */
export function isWasmReady(): boolean {
  if (!runtimeModule) {
    return false;
  }
  return runtimeModule.isWasmReady();
}

/**
 * Get the initialized crypto API
 * Throws if WASM is not ready
 */
export function getCryptoApi(): import('./types').PoseidonCrypto {
  if (!runtimeModule) {
    throw new Error('WASM not initialized. Call initWasm() first.');
  }
  return runtimeModule.getCryptoApi();
}
