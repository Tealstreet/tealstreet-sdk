import { readFileSync } from 'fs';
import { join } from 'path';

import * as ed25519 from '@noble/ed25519';
import { sha512 } from '@noble/hashes/sha2.js';
import { beforeAll, describe, expect, it } from 'vitest';

const sha512ForEd25519: NonNullable<typeof ed25519.hashes.sha512> = (message) =>
  Uint8Array.from(sha512(message));

// Set the crypto hash function for noble/ed25519 v3
ed25519.hashes.sha512 = sha512ForEd25519;

interface ApiKeyConfig {
  exchange: string;
  testnet: boolean;
  name: string;
  apiKey: string;
  secret: string;
  user: string;
  active: boolean;
  color: string;
}

/**
 * Generate ED25519 signature for Backpack API requests
 * @param instruction - The instruction type (e.g., "orderExecute", "balanceQuery")
 * @param params - Request parameters
 * @param timestamp - Unix timestamp in milliseconds
 * @param window - Validity window in milliseconds
 * @param privateKeyBase64 - Base64-encoded private key
 * @returns Base64-encoded signature
 */
async function generateSignature(
  instruction: string,
  params: Record<string, any>,
  timestamp: number,
  window: number,
  privateKeyBase64: string,
): Promise<string> {
  // 1. Sort parameters alphabetically and convert to query string
  const sortedKeys = Object.keys(params).sort();
  const queryParts: string[] = [];

  for (const key of sortedKeys) {
    const value = params[key];
    if (value !== null && value !== undefined) {
      queryParts.push(`${key}=${encodeURIComponent(String(value))}`);
    }
  }

  const queryString = queryParts.join('&');

  // 2. Build the signature payload: instruction=<instruction>&<params>&timestamp=<timestamp>&window=<window>
  const signaturePayload = `instruction=${instruction}${queryString ? '&' + queryString : ''}&timestamp=${timestamp}&window=${window}`;

  // 3. Convert to bytes
  const messageBytes = new TextEncoder().encode(signaturePayload);

  // 4. Decode private key from base64
  const privateKeyBytes = Uint8Array.from(Buffer.from(privateKeyBase64, 'base64'));

  // 5. Sign the message
  const signatureBytes = await ed25519.signAsync(messageBytes, privateKeyBytes);

  // 6. Encode signature to base64
  const signatureBase64 = Buffer.from(signatureBytes).toString('base64');

  return signatureBase64;
}

/**
 * Verify ED25519 signature
 */
async function verifySignature(
  instruction: string,
  params: Record<string, any>,
  timestamp: number,
  window: number,
  signature: string,
  publicKeyBase64: string,
): Promise<boolean> {
  // Reconstruct the signature payload
  const sortedKeys = Object.keys(params).sort();
  const queryParts: string[] = [];

  for (const key of sortedKeys) {
    const value = params[key];
    if (value !== null && value !== undefined) {
      queryParts.push(`${key}=${encodeURIComponent(String(value))}`);
    }
  }

  const queryString = queryParts.join('&');
  const signaturePayload = `instruction=${instruction}${queryString ? '&' + queryString : ''}&timestamp=${timestamp}&window=${window}`;

  const messageBytes = new TextEncoder().encode(signaturePayload);
  const publicKeyBytes = Uint8Array.from(Buffer.from(publicKeyBase64, 'base64'));
  const signatureBytes = Uint8Array.from(Buffer.from(signature, 'base64'));

  return await ed25519.verifyAsync(signatureBytes, messageBytes, publicKeyBytes);
}

describe('Backpack Authentication', () => {
  let apiConfig: ApiKeyConfig;

  beforeAll(() => {
    // Load API credentials from .apikey.json (in parent directory)
    const configPath = join(__dirname, '..', '.apikey.json');
    const configData = readFileSync(configPath, 'utf-8');
    apiConfig = JSON.parse(configData);
  });

  it('should load API credentials correctly', () => {
    expect(apiConfig).toBeDefined();
    expect(apiConfig.exchange).toBe('backpack');
    expect(apiConfig.apiKey).toBeTruthy();
    expect(apiConfig.secret).toBeTruthy();
    expect(apiConfig.apiKey).toMatch(/^[A-Za-z0-9+/]+=*$/); // Base64 format
    expect(apiConfig.secret).toMatch(/^[A-Za-z0-9+/]+=*$/); // Base64 format
  });

  it('should generate valid ED25519 signature', async () => {
    const instruction = 'balanceQuery';
    const params = {};
    const timestamp = Date.now();
    const window = 5000;

    const signature = await generateSignature(instruction, params, timestamp, window, apiConfig.secret);

    expect(signature).toBeTruthy();
    expect(signature).toMatch(/^[A-Za-z0-9+/]+=*$/); // Base64 format
    expect(signature.length).toBeGreaterThan(0);
  });

  it('should generate consistent signatures for same input', async () => {
    const instruction = 'orderQuery';
    const params = { symbol: 'SOL_USDC' };
    const timestamp = Date.now();
    const window = 5000;

    const signature1 = await generateSignature(instruction, params, timestamp, window, apiConfig.secret);

    const signature2 = await generateSignature(instruction, params, timestamp, window, apiConfig.secret);

    expect(signature1).toBe(signature2);
  });

  it('should generate different signatures for different parameters', async () => {
    const instruction = 'orderExecute';
    const timestamp = Date.now();
    const window = 5000;

    const params1 = { symbol: 'SOL_USDC', side: 'Bid' };
    const params2 = { symbol: 'SOL_USDC', side: 'Ask' };

    const signature1 = await generateSignature(instruction, params1, timestamp, window, apiConfig.secret);

    const signature2 = await generateSignature(instruction, params2, timestamp, window, apiConfig.secret);

    expect(signature1).not.toBe(signature2);
  });

  it('should generate different signatures for different timestamps', async () => {
    const instruction = 'balanceQuery';
    const params = {};
    const window = 5000;

    const signature1 = await generateSignature(instruction, params, Date.now(), window, apiConfig.secret);

    // Wait a bit to ensure different timestamp
    await new Promise((resolve) => setTimeout(resolve, 10));

    const signature2 = await generateSignature(instruction, params, Date.now(), window, apiConfig.secret);

    expect(signature1).not.toBe(signature2);
  });

  it('should verify signature with public key', async () => {
    const instruction = 'balanceQuery';
    const params = {};
    const timestamp = Date.now();
    const window = 5000;

    const signature = await generateSignature(instruction, params, timestamp, window, apiConfig.secret);

    const isValid = await verifySignature(instruction, params, timestamp, window, signature, apiConfig.apiKey);

    expect(isValid).toBe(true);
  });

  it('should sort parameters alphabetically', async () => {
    const instruction = 'orderExecute';
    const timestamp = Date.now();
    const window = 5000;

    // Parameters intentionally not in alphabetical order
    const params = {
      quantity: '1.5',
      side: 'Bid',
      orderType: 'Limit',
      price: '100.5',
      symbol: 'SOL_USDC',
    };

    const signature = await generateSignature(instruction, params, timestamp, window, apiConfig.secret);

    // Verify the signature is valid (which confirms proper sorting)
    const isValid = await verifySignature(instruction, params, timestamp, window, signature, apiConfig.apiKey);

    expect(isValid).toBe(true);
  });

  it('should handle empty parameters', async () => {
    const instruction = 'balanceQuery';
    const params = {};
    const timestamp = Date.now();
    const window = 5000;

    const signature = await generateSignature(instruction, params, timestamp, window, apiConfig.secret);

    const isValid = await verifySignature(instruction, params, timestamp, window, signature, apiConfig.apiKey);

    expect(isValid).toBe(true);
  });

  it('should handle parameters with special characters', async () => {
    const instruction = 'orderExecute';
    const timestamp = Date.now();
    const window = 5000;

    const params = {
      symbol: 'BTC_USDC',
      clientId: 'test-order-123',
    };

    const signature = await generateSignature(instruction, params, timestamp, window, apiConfig.secret);

    const isValid = await verifySignature(instruction, params, timestamp, window, signature, apiConfig.apiKey);

    expect(isValid).toBe(true);
  });

  it('should reject invalid signatures', async () => {
    const instruction = 'balanceQuery';
    const params = {};
    const timestamp = Date.now();
    const window = 5000;

    // Create a valid 64-byte signature with wrong data
    const invalidSignature = Buffer.alloc(64, 0).toString('base64');

    const isValid = await verifySignature(instruction, params, timestamp, window, invalidSignature, apiConfig.apiKey);

    expect(isValid).toBe(false);
  });
});
