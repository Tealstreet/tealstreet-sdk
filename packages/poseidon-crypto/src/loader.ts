/**
 * WASM Loader for Poseidon Crypto
 *
 * This module handles loading the WebAssembly binary and initializing
 * the Go runtime with the crypto functions.
 */

import type { PoseidonCrypto } from './types';

let wasmReady = false;
let wasmInitPromise: Promise<void> | null = null;

/**
 * Load and initialize the WASM module
 * @returns Promise that resolves when WASM is ready
 */
export async function initWasm(): Promise<void> {
  if (wasmReady) {
    return;
  }

  if (wasmInitPromise) {
    return wasmInitPromise;
  }

  wasmInitPromise = (async () => {
    try {
      // Dynamically import wasm_exec.js to avoid build issues
      const wasmExecModule = await import('./wasm/wasm_exec.js');

      // For Node.js environment
      if (typeof process !== 'undefined' && process.versions?.node) {
        const fs = await import('fs');
        const path = await import('path');
        const wasmPath = path.join(__dirname, 'wasm', 'poseidon_crypto.wasm');
        const wasmBuffer = fs.readFileSync(wasmPath);

        // @ts-ignore - Go global from wasm_exec.js
        const go = new (globalThis as any).Go();
        const result = await WebAssembly.instantiate(wasmBuffer, go.importObject);
        go.run(result.instance);

        // Wait for initialization
        await new Promise((resolve) => setTimeout(resolve, 100));
        wasmReady = true;
      }
      // For browser environment
      else if (typeof fetch !== 'undefined') {
        // In browser, fetch WASM from public folder
        // This will be served by Next.js from /public folder
        const baseUrl = typeof self !== 'undefined' && self.location?.origin ? self.location.origin : '';
        const response = await fetch(`${baseUrl}/wasm/poseidon_crypto.wasm`);
        if (!response.ok) {
          throw new Error(`Failed to fetch WASM: ${response.status} ${response.statusText}`);
        }
        const wasmBuffer = await response.arrayBuffer();

        // @ts-ignore - Go global from wasm_exec.js
        const go = new (globalThis as any).Go();
        const result = await WebAssembly.instantiate(wasmBuffer, go.importObject);
        go.run(result.instance);

        // Wait for initialization
        await new Promise((resolve) => setTimeout(resolve, 100));
        wasmReady = true;
      } else {
        throw new Error('Unsupported environment for WASM loading');
      }
    } catch (error) {
      wasmInitPromise = null;
      throw new Error(`Failed to initialize WASM: ${error}`);
    }
  })();

  return wasmInitPromise;
}

/**
 * Check if WASM is ready
 */
export function isWasmReady(): boolean {
  return wasmReady;
}

/**
 * Get the initialized crypto API
 * Throws if WASM is not ready
 */
export function getCryptoApi(): PoseidonCrypto {
  if (!wasmReady) {
    throw new Error('WASM not initialized. Call initWasm() first.');
  }

  return {
    isReady: () => wasmReady,
    poseidon2Hash: (input: string) => (globalThis as any).poseidon2Hash(input),
    poseidon2HashFp5: (input: string) => {
      const hashFp5Fn = (globalThis as any).poseidon2HashFp5 ?? (globalThis as any).poseidon2Hash;
      if (typeof hashFp5Fn !== 'function') {
        return { error: 'poseidon2HashFp5 function not available' };
      }
      return hashFp5Fn(input);
    },
    poseidon2HashTwoToOne: (hash1: string, hash2: string) => (globalThis as any).poseidon2HashTwoToOne(hash1, hash2),
    schnorrGenerateKeys: () => (globalThis as any).schnorrGenerateKeys(),
    schnorrPkFromSk: (privateKey: string) => (globalThis as any).schnorrPkFromSk(privateKey),
    schnorrSign: (message: string, privateKey: string) => (globalThis as any).schnorrSign(message, privateKey),
    schnorrVerify: (message: string, publicKey: string, signature: string) =>
      (globalThis as any).schnorrVerify(message, publicKey, signature),
  };
}
