/**
 * Example usage of @tealstreet/poseidon-crypto
 *
 * This file demonstrates how to use the Poseidon crypto library
 * in both Node.js and browser environments.
 */

import { getCryptoApi, initWasm } from './index';

async function main() {
  console.log('Initializing WASM...');

  // Initialize the WASM module
  await initWasm();

  console.log('WASM initialized successfully!\n');

  // Get the crypto API
  const crypto = getCryptoApi();

  // Example 1: Hash data
  console.log('=== Example 1: Poseidon2 Hash ===');
  const input = '0000000000000000000000000000000000000000000000000000000000000000';
  console.log('Input:', input);
  const hash = crypto.poseidon2Hash(input);
  console.log('Hash:', hash);
  console.log();

  // Example 2: Hash two-to-one
  console.log('=== Example 2: Hash Two-to-One ===');
  const hash1 = '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';
  const hash2 = 'fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210';
  console.log('Hash1:', hash1);
  console.log('Hash2:', hash2);
  const combined = crypto.poseidon2HashTwoToOne(hash1, hash2);
  console.log('Combined:', combined);
  console.log();

  // Example 3: Generate keys
  console.log('=== Example 3: Generate Schnorr Keys ===');
  const keys = crypto.schnorrGenerateKeys();
  if ('error' in keys) {
    console.error('Error generating keys:', keys.error);
    return;
  }
  console.log('Private Key:', keys.privateKey);
  console.log('Public Key:', keys.publicKey);
  console.log();

  // Example 4: Derive public key from private key
  console.log('=== Example 4: Derive Public Key ===');
  const derivedPk = crypto.schnorrPkFromSk(keys.privateKey);
  console.log('Derived PK:', derivedPk);
  console.log('Matches generated PK:', derivedPk === keys.publicKey);
  console.log();

  // Example 5: Sign and verify
  console.log('=== Example 5: Sign and Verify ===');
  const message = '0000000000000000000000000000000000000000000000000000000000000001';
  console.log('Message:', message);

  const signature = crypto.schnorrSign(message, keys.privateKey);
  if (typeof signature === 'object' && 'error' in signature) {
    console.error('Error signing:', signature.error);
    return;
  }
  console.log('Signature:', signature);

  const isValid = crypto.schnorrVerify(message, keys.publicKey, signature);
  console.log('Signature valid:', isValid);

  // Verify with wrong key (should fail)
  const wrongKeys = crypto.schnorrGenerateKeys();
  if ('error' in wrongKeys) {
    console.error('Error generating wrong keys:', wrongKeys.error);
    return;
  }
  const invalidVerify = crypto.schnorrVerify(message, wrongKeys.publicKey, signature);
  console.log('Wrong key verification (should be false):', invalidVerify);
}

// Run the example
if (require.main === module) {
  main().catch((error) => {
    console.error('Fatal error:', error);
    process.exit(1);
  });
}

export { main };
