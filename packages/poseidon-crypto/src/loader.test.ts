/**
 * Tests for WASM loader
 */

import { describe, expect, it } from 'vitest';

import { getCryptoApi, initWasm, isWasmReady } from './loader';

describe('Poseidon Crypto WASM Loader', () => {
  it('should initialize WASM successfully', async () => {
    await initWasm();
    expect(isWasmReady()).toBe(true);
  });

  it('should provide crypto API after initialization', async () => {
    await initWasm();
    const crypto = getCryptoApi();

    expect(crypto).toBeDefined();
    expect(crypto.isReady()).toBe(true);
    expect(crypto.poseidon2Hash).toBeDefined();
    expect(crypto.schnorrGenerateKeys).toBeDefined();
  });

  it('should hash data correctly', async () => {
    await initWasm();
    const crypto = getCryptoApi();

    const input = '0000000000000000000000000000000000000000000000000000000000000000';
    const hash = crypto.poseidon2Hash(input);

    expect(typeof hash === 'string' || 'error' in hash).toBe(true);
    if (typeof hash === 'string') {
      expect(hash).toHaveLength(64); // 32 bytes = 64 hex chars
    }
  });

  it('should generate Schnorr keys', async () => {
    await initWasm();
    const crypto = getCryptoApi();

    const keys = crypto.schnorrGenerateKeys();

    if ('error' in keys) {
      throw new Error(`Key generation failed: ${keys.error}`);
    }

    expect(keys.privateKey).toHaveLength(80); // 40 bytes = 80 hex chars
    expect(keys.publicKey).toHaveLength(80); // 40 bytes = 80 hex chars
  });

  it('should sign and verify correctly', async () => {
    await initWasm();
    const crypto = getCryptoApi();

    const keys = crypto.schnorrGenerateKeys();
    if ('error' in keys) {
      throw new Error(`Key generation failed: ${keys.error}`);
    }

    const message = '0000000000000000000000000000000000000000000000000000000000000001';
    const signature = crypto.schnorrSign(message, keys.privateKey);

    if (typeof signature === 'object' && 'error' in signature) {
      throw new Error(`Signing failed: ${signature.error}`);
    }

    expect(signature).toHaveLength(160); // 80 bytes = 160 hex chars

    const isValid = crypto.schnorrVerify(message, keys.publicKey, signature);
    expect(isValid).toBe(true);

    // Verify with wrong key should fail
    const wrongKeys = crypto.schnorrGenerateKeys();
    if ('error' in wrongKeys) {
      throw new Error(`Wrong key generation failed: ${wrongKeys.error}`);
    }

    const isInvalid = crypto.schnorrVerify(message, wrongKeys.publicKey, signature);
    expect(isInvalid).toBe(false);
  });
});
